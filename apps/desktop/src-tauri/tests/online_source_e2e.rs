use std::thread;
use std::time::{Duration, Instant};

use lumina_lib::player::mpv::LibMpvPlayer;
use lumina_lib::ytdl::YtdlService;

const DEFAULT_PUBLIC_MP4: &str =
    "https://test-videos.co.uk/vids/bigbuckbunny/mp4/h264/720/Big_Buck_Bunny_720_10s_1MB.mp4";

#[test]
fn public_online_source_resolves_and_reaches_mpv_demux() {
    let page_url = std::env::var("LUMINA_ONLINE_E2E_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_PUBLIC_MP4.to_string());

    let ytdl = YtdlService::new();
    let resolved = ytdl
        .resolve(&page_url)
        .expect("yt-dlp should resolve the public online video without cookies");
    assert!(
        resolved
            .title
            .as_deref()
            .is_some_and(|title| !title.trim().is_empty()),
        "online resolution should include a title"
    );
    assert!(
        !resolved.formats.is_empty(),
        "online resolution should include at least one format"
    );

    let target = ytdl
        .play_target(&page_url, None)
        .expect("resolved online video should produce a playback target");
    assert!(
        target.stream_url.starts_with("https://"),
        "playback target should contain an HTTPS stream URL"
    );

    let player = LibMpvPlayer::initialize().expect("headless libmpv should initialize");
    player
        .open(&target.stream_url)
        .expect("libmpv should accept the resolved online stream");

    let deadline = Instant::now() + Duration::from_secs(45);
    let mut demux_ready = false;
    while Instant::now() < deadline {
        player.drain_diagnostic_events();
        let duration_ms = player.duration_ms().unwrap_or_default();
        let position_ms = player.position_ms().unwrap_or_default();
        if duration_ms > 0 && position_ms > 0 {
            demux_ready = true;
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }

    assert!(
        demux_ready,
        "libmpv should demux the online stream and advance playback within 45 seconds"
    );
    player
        .stop()
        .expect("libmpv should stop after the E2E playback check");
}
