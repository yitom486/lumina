//! Fake-child E2E for the ACP session resume paths in
//! `open_session_on_live_process` (`runtime/lifecycle.rs`).
//!
//! The legacy mock agent (`tests/bin/lumina_acp_test_mock_agent.rs`) never
//! advertises the resume capability; these tests prove a valid in-scope hint
//! still earns a real `session/resume` round-trip against a live child, and
//! that a failed resume honestly falls back to `session/new` with the
//! classified outcome. Spawn/teardown paradigm mirrors
//! `chapter_session_blackbox.rs`; no second framework.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use lumina_acp::{
    AcpClientSettings, AcpEvent, AcpService, AgentKind, AgentProfileInput, AgentProfilesHint,
    ResumeOutcome, SavedSessionHint, SessionEnvironment, SessionKind,
};

/// One live mock child at a time: the no-residual-process check below can
/// only attribute `tasklist` rows while a single test owns the child, and
/// `set_default_environment` is process-global (`OnceLock`).
static SERIAL: Mutex<()> = Mutex::new(());

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new(tag: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = std::env::temp_dir().join(format!(
            "lumina-acp-session-resume-blackbox-{tag}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path)?;
        Ok(Self(path))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct TestSessionEnvironment {
    root: PathBuf,
}

impl SessionEnvironment for TestSessionEnvironment {
    fn snapshot_path(&self, _cwd: &Path, kind: SessionKind) -> PathBuf {
        self.root.join(format!("snapshot-{}.json", kind.as_str()))
    }

    fn sync_snapshot(&self, snapshot_path: &Path, vision_capable: bool) -> Result<(), String> {
        fs::write(
            snapshot_path,
            serde_json::json!({ "test": true, "visionCapable": vision_capable }).to_string(),
        )
        .map_err(|error| error.to_string())
    }

    fn mcp_servers(&self, _snapshot_path: &Path, _kind: SessionKind) -> serde_json::Value {
        serde_json::json!([])
    }

    fn snapshot_vision_capable(&self, _workspace: &Path) -> Option<bool> {
        None
    }
}

static SHARED_ENV_ROOT: OnceLock<PathBuf> = OnceLock::new();

/// The session environment is process-global; every test in this binary
/// shares one snapshot root while state/log files stay per-test.
fn install_shared_environment() -> Result<(), Box<dyn std::error::Error>> {
    let root = SHARED_ENV_ROOT
        .get_or_init(|| {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or(0);
            let path = std::env::temp_dir().join(format!(
                "lumina-acp-session-resume-blackbox-shared-{}-{nonce}",
                std::process::id()
            ));
            let _ = fs::create_dir_all(&path);
            path
        })
        .clone();
    let _ = lumina_acp::set_default_environment(Arc::new(TestSessionEnvironment { root }));
    Ok(())
}

struct ResumeFixture {
    dir: TestDirectory,
    state_path: PathBuf,
    log_path: PathBuf,
    cwd: String,
}

impl ResumeFixture {
    fn new(tag: &str) -> Result<Self, Box<dyn std::error::Error>> {
        install_shared_environment()?;
        let dir = TestDirectory::new(tag)?;
        let cwd = dir.path().to_string_lossy().into_owned();
        Ok(Self {
            state_path: dir.path().join("spawn-count.txt"),
            log_path: dir.path().join("mock-calls.log"),
            cwd,
            dir,
        })
    }

    fn hint(&self, session_id: &str) -> SavedSessionHint {
        SavedSessionHint {
            session_id: session_id.to_string(),
            profile_id: "mock".to_string(),
            cwd: self.cwd.clone(),
        }
    }

    fn profiles(
        &self,
        script: Option<&str>,
    ) -> Result<AgentProfilesHint, Box<dyn std::error::Error>> {
        let mock_agent = mock_agent_path()?;
        let mut env = HashMap::from([
            (
                "LUMINA_ACP_MOCK_STATE".to_string(),
                self.state_path.to_string_lossy().into_owned(),
            ),
            (
                "LUMINA_ACP_MOCK_LOG".to_string(),
                self.log_path.to_string_lossy().into_owned(),
            ),
        ]);
        if let Some(script) = script {
            env.insert("LUMINA_ACP_MOCK_SCRIPT".to_string(), script.to_string());
        }
        Ok(AgentProfilesHint {
            active_profile_id: "mock".into(),
            profiles: vec![AgentProfileInput {
                id: "mock".into(),
                name: "测试 ACP Agent".into(),
                kind: AgentKind::Custom,
                command: mock_agent,
                args: Vec::new(),
                env,
                launcher: None,
                env_preset: None,
                auth_policy: None,
                auth_methods: Vec::new(),
                session_storage: None,
            }],
        })
    }

    fn mock_calls(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let text = fs::read_to_string(&self.log_path)?;
        Ok(text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect())
    }
}

fn mock_agent_path() -> Result<String, Box<dyn std::error::Error>> {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_lumina_acp_test_mock_agent") {
        return Ok(path);
    }
    let mut path = std::env::current_exe()?;
    path.pop();
    path.pop();
    path.push(format!(
        "lumina-acp-test-mock-agent{}",
        std::env::consts::EXE_SUFFIX
    ));
    if path.is_file() {
        return Ok(path.to_string_lossy().into_owned());
    }
    Err(format!("test mock agent binary not found at {}", path.display()).into())
}

struct SavedObservation {
    session_id: String,
    profile_id: String,
    resume: Option<ResumeOutcome>,
}

fn connect_and_capture(
    cwd: &str,
    hint: Option<SavedSessionHint>,
    profiles: &AgentProfilesHint,
) -> Result<(AcpService, Vec<AcpEvent>), Box<dyn std::error::Error>> {
    let service = AcpService::new();
    let mut events = Vec::new();
    service
        .connect(
            Some(cwd.to_string()),
            Some("mock".to_string()),
            hint,
            AcpClientSettings::default(),
            profiles.clone(),
            |event| events.push(event),
        )
        .map_err(|error| format!("connect failed: {} ({:?})", error.message, error.code))?;
    Ok((service, events))
}

fn saved_observation(
    events: &[AcpEvent],
    context: &str,
) -> Result<SavedObservation, Box<dyn std::error::Error>> {
    for event in events {
        if let AcpEvent::SessionSaved {
            session_id,
            profile_id,
            cwd: _,
            resume,
        } = event
        {
            return Ok(SavedObservation {
                session_id: session_id.clone(),
                profile_id: profile_id.clone(),
                resume: *resume,
            });
        }
    }
    Err(format!("{context}: no SessionSaved event; events={events:?}").into())
}

/// Shared teardown: close the live session, prove the slot is empty and the
/// mock child is really gone, then prove the temp files are really gone.
fn teardown(
    service: &AcpService,
    profiles: &AgentProfilesHint,
    fixture: ResumeFixture,
    context: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    service.close_session().map_err(|error| {
        format!(
            "{context}: close_session failed: {} ({:?})",
            error.message, error.code
        )
    })?;
    assert!(
        !service.status(profiles).session_active,
        "{context}: session slot must be empty after close_session"
    );
    wait_mock_agent_gone(context);
    let path = fixture.dir.path().to_path_buf();
    drop(fixture);
    assert!(
        !path.exists(),
        "{context}: fixture temp dir must be removed, still present at {}",
        path.display()
    );
    Ok(())
}

fn wait_mock_agent_gone(context: &str) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while mock_agent_alive() {
        if Instant::now() >= deadline {
            panic!("{context}: mock agent child still alive 15s after close_session; child leaked");
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

#[cfg(windows)]
fn mock_agent_alive() -> bool {
    let output = std::process::Command::new("tasklist")
        .args([
            "/FI",
            "IMAGENAME eq lumina-acp-test-mock-agent.exe",
            "/FO",
            "CSV",
            "/NH",
        ])
        .output()
        .expect("tasklist must run for the no-residual-process check");
    String::from_utf8_lossy(&output.stdout).contains("lumina-acp-test-mock-agent.exe")
}

#[cfg(not(windows))]
fn mock_agent_alive() -> bool {
    match std::process::Command::new("pgrep")
        .args(["-f", "lumina-acp-test-mock-agent"])
        .output()
    {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

#[test]
fn resume_success_returns_original_id() -> Result<(), Box<dyn std::error::Error>> {
    let _serial = SERIAL.lock().expect("resume blackbox serial lock poisoned");
    let fixture = ResumeFixture::new("resume-ok")?;
    let profiles = fixture.profiles(Some("resume-ok"))?;
    let hint_id = "saved-chat-abc123";

    let (service, events) =
        connect_and_capture(&fixture.cwd.clone(), Some(fixture.hint(hint_id)), &profiles)?;
    let saved = saved_observation(&events, "resume success")?;
    assert_eq!(
        saved.session_id, hint_id,
        "resume must return the original id"
    );
    assert_eq!(saved.profile_id, "mock", "resume must keep the profile");
    assert_eq!(
        saved.resume,
        Some(ResumeOutcome::Resumed),
        "resume must report Resumed"
    );

    let calls = fixture.mock_calls()?;
    assert!(
        calls.contains(&"session/resume".to_string()),
        "resume must call session/resume, got {calls:?}"
    );
    assert!(
        !calls.contains(&"session/new".to_string()),
        "resume success must not call session/new, got {calls:?}"
    );

    teardown(&service, &profiles, fixture, "resume success")
}

#[test]
fn resume_attempted_without_advertised_capability() -> Result<(), Box<dyn std::error::Error>> {
    // The mock's initialize only advertises `close`, never `resume`, and no
    // script is selected here (legacy chapter behavior). A valid in-scope
    // hint must still earn a session/resume attempt: capability advertisement
    // is decoupled from the attempt decision (see should_attempt_resume).
    let _serial = SERIAL.lock().expect("resume blackbox serial lock poisoned");
    let fixture = ResumeFixture::new("resume-unadvertised")?;
    let profiles = fixture.profiles(None)?;
    let hint_id = "saved-chat-legacy";

    let (service, events) =
        connect_and_capture(&fixture.cwd.clone(), Some(fixture.hint(hint_id)), &profiles)?;
    let saved = saved_observation(&events, "unadvertised resume")?;
    assert_eq!(
        saved.session_id, hint_id,
        "resume must return the original id"
    );
    assert_eq!(
        saved.resume,
        Some(ResumeOutcome::Resumed),
        "resume must report Resumed"
    );

    let calls = fixture.mock_calls()?;
    assert!(
        calls.contains(&"session/resume".to_string()),
        "session/resume must be attempted even without the advertised capability, got {calls:?}"
    );

    teardown(&service, &profiles, fixture, "unadvertised resume")
}

#[test]
fn resume_failure_unavailable_creates_new_session() -> Result<(), Box<dyn std::error::Error>> {
    let _serial = SERIAL.lock().expect("resume blackbox serial lock poisoned");
    let fixture = ResumeFixture::new("resume-unavailable")?;
    let profiles = fixture.profiles(Some("resume-fail-unavailable"))?;
    let hint_id = "saved-chat-gone";

    let (service, events) =
        connect_and_capture(&fixture.cwd.clone(), Some(fixture.hint(hint_id)), &profiles)?;
    let saved = saved_observation(&events, "unavailable fallback")?;
    assert_ne!(
        saved.session_id, hint_id,
        "failed resume must not reuse the stale id"
    );
    assert_eq!(
        saved.session_id, "mock-session-1",
        "fallback must come from session/new"
    );
    assert_eq!(
        saved.resume,
        Some(ResumeOutcome::Unavailable),
        "lost conversation must report Unavailable"
    );

    let calls = fixture.mock_calls()?;
    let resume_pos = calls
        .iter()
        .position(|call| call == "session/resume")
        .expect("resume fallback must attempt session/resume first");
    let new_pos = calls
        .iter()
        .position(|call| call == "session/new")
        .expect("resume fallback must then call session/new");
    assert!(
        resume_pos < new_pos,
        "session/resume must precede session/new, got {calls:?}"
    );

    teardown(&service, &profiles, fixture, "unavailable fallback")
}

#[test]
fn resume_failure_occupied_reports_occupied() -> Result<(), Box<dyn std::error::Error>> {
    let _serial = SERIAL.lock().expect("resume blackbox serial lock poisoned");
    let fixture = ResumeFixture::new("resume-occupied")?;
    let profiles = fixture.profiles(Some("resume-fail-occupied"))?;
    let hint_id = "saved-chat-held";

    let (service, events) =
        connect_and_capture(&fixture.cwd.clone(), Some(fixture.hint(hint_id)), &profiles)?;
    let saved = saved_observation(&events, "occupied fallback")?;
    assert_ne!(
        saved.session_id, hint_id,
        "occupied resume must not reuse the held id"
    );
    assert_eq!(
        saved.session_id, "mock-session-1",
        "fallback must come from session/new"
    );
    assert_eq!(
        saved.resume,
        Some(ResumeOutcome::Occupied),
        "held conversation must report Occupied, not Unavailable"
    );

    let calls = fixture.mock_calls()?;
    assert!(
        calls.contains(&"session/resume".to_string()),
        "occupied path must attempt session/resume, got {calls:?}"
    );
    assert!(
        calls.contains(&"session/new".to_string()),
        "occupied path must fall back to session/new, got {calls:?}"
    );

    teardown(&service, &profiles, fixture, "occupied fallback")
}

#[test]
fn missing_hint_goes_straight_to_new() -> Result<(), Box<dyn std::error::Error>> {
    let _serial = SERIAL.lock().expect("resume blackbox serial lock poisoned");
    let fixture = ResumeFixture::new("no-hint")?;
    let profiles = fixture.profiles(Some("resume-ok"))?;

    let (service, events) = connect_and_capture(&fixture.cwd.clone(), None, &profiles)?;
    let saved = saved_observation(&events, "no hint")?;
    assert_eq!(
        saved.session_id, "mock-session-1",
        "no-hint connect must come from session/new"
    );
    assert_eq!(saved.resume, None, "no-hint connect attempts no restore");

    let calls = fixture.mock_calls()?;
    assert!(
        !calls.contains(&"session/resume".to_string()),
        "no-hint connect must skip session/resume, got {calls:?}"
    );
    assert!(
        calls.contains(&"session/new".to_string()),
        "no-hint connect must call session/new, got {calls:?}"
    );

    teardown(&service, &profiles, fixture, "no hint")
}

#[test]
fn out_of_scope_hint_goes_straight_to_new() -> Result<(), Box<dyn std::error::Error>> {
    let _serial = SERIAL.lock().expect("resume blackbox serial lock poisoned");
    let fixture = ResumeFixture::new("scope-mismatch")?;
    let profiles = fixture.profiles(Some("resume-ok"))?;
    let foreign_hint = SavedSessionHint {
        session_id: "saved-chat-elsewhere".to_string(),
        profile_id: "another-agent".to_string(),
        cwd: fixture.cwd.clone(),
    };

    let (service, events) =
        connect_and_capture(&fixture.cwd.clone(), Some(foreign_hint), &profiles)?;
    let saved = saved_observation(&events, "scope mismatch")?;
    assert_eq!(
        saved.session_id, "mock-session-1",
        "out-of-scope hint must come from session/new"
    );
    assert_eq!(saved.resume, None, "out-of-scope hint attempts no restore");

    let calls = fixture.mock_calls()?;
    assert!(
        !calls.contains(&"session/resume".to_string()),
        "out-of-scope hint must skip session/resume, got {calls:?}"
    );
    assert!(
        calls.contains(&"session/new".to_string()),
        "out-of-scope hint must call session/new, got {calls:?}"
    );

    teardown(&service, &profiles, fixture, "scope mismatch")
}
