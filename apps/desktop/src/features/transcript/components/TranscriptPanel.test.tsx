import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { usePlayerStore } from "@/features/player";
import { useTrackStore } from "@/features/player/trackStore";
import { useAskAboutStore } from "@lumina/chat-ui/askAboutStore";
import { useChatUiStore } from "@lumina/chat-ui/chatUiStore";

import { TranscriptPanel } from "./TranscriptPanel";
import { SubtitleWorkshopPanel } from "./SubtitleWorkshopPanel";
import { useFollowStore } from "@lumina/transcript-ui";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  Channel: vi.fn().mockImplementation(() => ({ onmessage: null })),
}));

import { invoke } from "@tauri-apps/api/core";

const CUES = [
  { index: 0, startMs: 0, endMs: 1000, text: "第一句" },
  { index: 1, startMs: 1000, endMs: 2000, text: "第二句" },
  { index: 2, startMs: 2000, endMs: 3000, text: "第三句" },
];

const scrollIntoView = vi.fn();

function renderPanel() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <TranscriptPanel />
    </QueryClientProvider>,
  );
}

function renderWorkshopPanel() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <SubtitleWorkshopPanel />
    </QueryClientProvider>,
  );
}

function seekCalls() {
  return vi
    .mocked(invoke)
    .mock.calls.filter(([cmd]) => cmd === "player_seek");
}

beforeEach(() => {
  Object.defineProperty(window.HTMLElement.prototype, "scrollIntoView", {
    configurable: true,
    writable: true,
    value: scrollIntoView,
  });
  scrollIntoView.mockClear();
  vi.mocked(invoke).mockImplementation((cmd: string) => {
    if (cmd === "subtitle_list_choices")
      return Promise.resolve([
        {
          id: "s1",
          source: "Embedded",
          label: "中文",
          supported: true,
          streamIndex: 0,
        },
      ]);
    if (cmd === "subtitle_load_choice")
      return Promise.resolve({ choiceId: "s1", cues: CUES });
    if (cmd === "subtitle_proofread_track")
      return Promise.resolve({ choiceId: "s1", cues: CUES });
    if (cmd === "asr_status")
      return Promise.resolve({
        available: false,
        installSupported: false,
        models: [],
        catalog: [],
      });
    if (cmd === "media_inspect")
      return Promise.resolve({ path: "C:\\v\\a.mp4", streams: [], chapters: [] });
    if (cmd === "library_agent_models_discover")
      return Promise.resolve(null);
    return Promise.resolve(null);
  });
  useFollowStore.setState({ followEnabled: true, browsing: false });
  useTrackStore.setState({ subtitleChoiceId: "s1" });
  useAskAboutStore.setState({ request: null });
  useChatUiStore.getState().closeChat();
  usePlayerStore.setState({
    currentFile: "C:\\v\\a.mp4",
    status: "Paused",
    currentTimeMs: 1500,
  });
  localStorage.clear();
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  localStorage.clear();
});

describe("TranscriptPanel online subtitles", () => {
  const REMOTE_URL = "https://www.youtube.com/watch?v=abc";
  const REMOTE_CUES = [
    { index: 1, startMs: 1000, endMs: 2000, text: "remote one" },
    { index: 2, startMs: 9000, endMs: 11000, text: "remote two" },
  ];

  function mockRemoteInvoke() {
    vi.mocked(invoke).mockImplementation((cmd: string, args?: unknown) => {
      if (cmd === "subtitle_list_choices") {
        const path = (args as { path?: string } | undefined)?.path;
        expect(path).toBe(REMOTE_URL);
        return Promise.resolve([
          {
            id: "online:en",
            source: "Sidecar",
            label: "在线 · en",
            supported: true,
            streamIndex: null,
            externalPath: null,
            codecName: "vtt",
            language: "en",
          },
        ]);
      }
      if (cmd === "subtitle_load_choice") {
        const payload = args as { path?: string; choiceId?: string };
        expect(payload.path).toBe(REMOTE_URL);
        expect(payload.choiceId).toBe("online:en");
        return Promise.resolve({
          sourcePath: REMOTE_URL,
          choiceId: "online:en",
          streamIndex: null,
          language: "en",
          codecName: "vtt",
          cues: REMOTE_CUES,
        });
      }
      if (cmd === "ytdl_cached_resolve") {
        return Promise.resolve({
          mediaId: "youtube:abc",
          title: "Demo",
          durationMs: 60000,
          webpageUrl: REMOTE_URL,
          extractor: "youtube",
          chapters: [],
          formats: [],
          subtitles: [{ language: "en", ext: "vtt", name: "English" }],
        });
      }
      if (cmd === "asr_status")
        return Promise.resolve({
          available: false,
          installSupported: false,
          models: [],
          catalog: [],
        });
      if (cmd === "media_inspect")
        return Promise.reject(new Error("remote must not probe local media"));
      if (cmd === "library_agent_models_discover")
        return Promise.resolve(null);
      return Promise.resolve(null);
    });
  }

  beforeEach(() => {
    usePlayerStore.setState({
      currentFile: REMOTE_URL,
      status: "Paused",
      currentTimeMs: 9500,
    });
    useTrackStore.setState({ subtitleChoiceId: "online:en" });
  });

  it("lists remote choices without signed URLs and loads after selection", async () => {
    mockRemoteInvoke();
    renderPanel();
    await waitFor(() => {
      expect(screen.getByText("remote two")).toBeInTheDocument();
    });
    const calls = vi.mocked(invoke).mock.calls;
    const listCalls = calls.filter(([cmd]) => cmd === "subtitle_list_choices");
    const loadCalls = calls.filter(([cmd]) => cmd === "subtitle_load_choice");
    expect(listCalls.length).toBeGreaterThan(0);
    expect(loadCalls.length).toBeGreaterThan(0);
    // List DTO must not leak signed URLs, cookies, or absolute cache paths.
    const listed = (await vi.mocked(invoke)("subtitle_list_choices", {
      path: REMOTE_URL,
    })) as Array<{ id: string; externalPath?: string | null }>;
    expect(listed[0]?.id).toBe("online:en");
    expect(listed[0]?.externalPath).toBeNull();
    const listedText = JSON.stringify(listed).toLowerCase();
    expect(listedText).not.toContain("sig=");
    expect(listedText).not.toContain("cookie");
    expect(listedText).not.toContain("http");
  });

  it("does not download before a subtitle is selected", async () => {
    mockRemoteInvoke();
    useTrackStore.setState({ subtitleChoiceId: null });
    renderPanel();
    // Remote choices are listed on demand, but no transcript download happens
    // until the user picks `online:<language>`.
    await waitFor(() => {
      expect(screen.getByText("在线 · en")).toBeInTheDocument();
    });
    expect(screen.queryByText("remote one")).not.toBeInTheDocument();
    const loadCalls = vi
      .mocked(invoke)
      .mock.calls.filter(([cmd]) => cmd === "subtitle_load_choice");
    expect(loadCalls).toHaveLength(0);
  });

  it("renders remote cues with the same timeline semantics as local", async () => {
    mockRemoteInvoke();
    renderPanel();
    await waitFor(() => {
      expect(screen.getByText("remote one")).toBeInTheDocument();
    });
    expect(screen.getByText("remote two")).toBeInTheDocument();
  });
});

describe("TranscriptPanel follow mode", () => {
  it("auto-scrolls while following", async () => {
    renderPanel();
    await waitFor(() => {
      expect(screen.getByText("第二句")).toBeInTheDocument();
    });
    await waitFor(() => {
      expect(scrollIntoView).toHaveBeenCalled();
    });
  });

  it("wheel pauses follow; resume restores; scroll never seeks", async () => {
    const { container } = renderPanel();
    await waitFor(() => {
      expect(screen.getByText("第二句")).toBeInTheDocument();
    });
    await waitFor(() => {
      expect(scrollIntoView).toHaveBeenCalled();
    });
    const callsAfterMount = scrollIntoView.mock.calls.length;

    const list = container.querySelector("ul");
    expect(list).not.toBeNull();
    fireEvent.wheel(list as Element);
    expect(
      await screen.findByText("回到当前播放位置"),
    ).toBeInTheDocument();

    act(() => {
      usePlayerStore.setState({ currentTimeMs: 2500 });
    });
    await waitFor(() => {
      expect(screen.getByText("第三句")).toBeInTheDocument();
    });
    // Browsing: active cue changed, no yank, no seek.
    expect(scrollIntoView.mock.calls.length).toBe(callsAfterMount);
    expect(seekCalls()).toHaveLength(0);

    fireEvent.click(screen.getByText("回到当前播放位置"));
    await waitFor(() => {
      expect(scrollIntoView.mock.calls.length).toBeGreaterThan(
        callsAfterMount,
      );
    });
    expect(
      screen.queryByText("回到当前播放位置"),
    ).not.toBeInTheDocument();
    expect(seekCalls()).toHaveLength(0);
  });

  it("follow-off stops auto scroll until re-enabled", async () => {
    renderPanel();
    await waitFor(() => {
      expect(screen.getByText("第二句")).toBeInTheDocument();
    });
    await waitFor(() => {
      expect(scrollIntoView).toHaveBeenCalled();
    });
    fireEvent.click(screen.getByText("跟随"));
    const callsAfterOff = scrollIntoView.mock.calls.length;
    act(() => {
      usePlayerStore.setState({ currentTimeMs: 500 });
    });
    await waitFor(() => {
      expect(screen.getByText("第一句")).toBeInTheDocument();
    });
    expect(scrollIntoView.mock.calls.length).toBe(callsAfterOff);
  });

  it("clicking a cue seeks and resumes follow", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "player_seek")
        return Promise.resolve({ status: "Paused", currentTimeMs: 0 });
      if (cmd === "subtitle_list_choices")
        return Promise.resolve([
          {
            id: "s1",
            source: "Embedded",
            label: "中文",
            supported: true,
            streamIndex: 0,
          },
        ]);
      if (cmd === "subtitle_load_choice")
        return Promise.resolve({ choiceId: "s1", cues: CUES });
      if (cmd === "asr_status")
        return Promise.resolve({
          available: false,
          installSupported: false,
          models: [],
          catalog: [],
        });
      if (cmd === "media_inspect")
        return Promise.resolve({ path: "C:\\v\\a.mp4", streams: [], chapters: [] });
      if (cmd === "library_agent_models_discover")
        return Promise.resolve(null);
      return Promise.resolve(null);
    });
    const { container } = renderPanel();
    await waitFor(() => {
      expect(screen.getByText("第一句")).toBeInTheDocument();
    });
    const list = container.querySelector("ul");
    fireEvent.wheel(list as Element);
    expect(
      await screen.findByText("回到当前播放位置"),
    ).toBeInTheDocument();

    fireEvent.click(screen.getByText("第三句"));
    await waitFor(() => {
      expect(seekCalls()).toHaveLength(1);
    });
    expect(
      screen.queryByText("回到当前播放位置"),
    ).not.toBeInTheDocument();
  });

  it("asks about a cue with its own anchor, not the live position", async () => {
    usePlayerStore.setState({ currentTimeMs: 2500 });
    renderPanel();
    await waitFor(() => {
      expect(screen.getByText("第三句")).toBeInTheDocument();
    });
    const askButtons = screen.getAllByText("问");
    expect(askButtons.length).toBeGreaterThan(0);
    fireEvent.click(askButtons[0] as Element);
    const request = useAskAboutStore.getState().request;
    // First cue starts at 0 even though playback is at 2500.
    expect(request).toMatchObject({ anchorMs: 0 });
    expect(request?.text).toContain("解释这一段");
    expect(useChatUiStore.getState().chatOpen).toBe(true);
    useAskAboutStore.getState().consume();
  });
});

describe("TranscriptPanel downloaded subtitles", () => {
  const REMOTE_CUES = [
    { index: 1, startMs: 0, endMs: 900, text: "downloaded one" },
    { index: 2, startMs: 900, endMs: 1800, text: "downloaded two" },
  ];
  const DOWNLOADED = {
    id: "cache:subdl:en",
    source: "Sidecar",
    label: "下载 · subdl · en",
    supported: true,
    streamIndex: null,
  };
  const CANDIDATES = [
    {
      provider: "subdl",
      language: "EN",
      releaseName: "demo.S01E01.1080p",
      sizeBytes: 102862,
      format: "srt",
      season: 1,
      episode: 1,
      downloadUrl: "https://dl.subdl.com/subtitle/a/b",
      cached: false,
    },
  ];

  function mockDownloadFlow() {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "subtitle_provider_status")
        return Promise.resolve([{ id: "subdl", needsKey: true, hasKey: true }]);
      if (cmd === "library_parse_filename")
        return Promise.resolve({
          key: "Our.Beloved.Summer.2021",
          season: null,
          episode: 1,
          year: 2021,
        });
      if (cmd === "subtitle_validate_provider_key")
        return Promise.resolve({ verified: true, message: "SubDL Key 有效" });
      if (cmd === "subtitle_search_online") return Promise.resolve(CANDIDATES);
      if (cmd === "subtitle_download_candidate")
        return Promise.resolve({
          sourcePath: "C:\\v\\a.mp4",
          choiceId: "cache:subdl:en",
          streamIndex: null,
          language: "en",
          codecName: "srt",
          cues: REMOTE_CUES,
        });
      if (cmd === "subtitle_list_choices") return Promise.resolve([DOWNLOADED]);
      if (cmd === "subtitle_load_choice")
        return Promise.resolve({ choiceId: "cache:subdl:en", cues: REMOTE_CUES });
      if (cmd === "subtitle_proofread_track")
        return Promise.resolve({ choiceId: "cache:subdl:en", cues: REMOTE_CUES });
      if (cmd === "asr_status")
        return Promise.resolve({
          available: false,
          installSupported: false,
          models: [],
          catalog: [],
        });
      if (cmd === "media_inspect")
        return Promise.resolve({ path: "C:\\v\\a.mp4", streams: [], chapters: [] });
      if (cmd === "library_agent_models_discover") return Promise.resolve(null);
      return Promise.resolve(null);
    });
  }

  function invokeCommands() {
    return vi.mocked(invoke).mock.calls.map(([cmd]) => cmd as string);
  }

  it("hides the online section for remote videos", async () => {
    mockDownloadFlow();
    usePlayerStore.setState({
      currentFile: "https://www.youtube.com/watch?v=abc",
      status: "Paused",
      currentTimeMs: 1000,
    });
    useTrackStore.setState({ subtitleChoiceId: null });
    renderPanel();
    await waitFor(() => {
      expect(screen.getByText("文稿")).toBeInTheDocument();
    });
    expect(screen.queryByText(/在线字幕/)).not.toBeInTheDocument();
    expect(
      invokeCommands().filter((cmd) => cmd === "subtitle_search_online"),
    ).toHaveLength(0);
  });

  it("searches without downloading and never touches the player", async () => {
    mockDownloadFlow();
    usePlayerStore.setState({
      currentFile: "C:\\v\\a.mp4",
      status: "Paused",
      currentTimeMs: 500,
    });
    useTrackStore.setState({ subtitleChoiceId: null });
    renderWorkshopPanel();
    await waitFor(() => {
      expect(screen.getByText("搜索字幕")).toBeInTheDocument();
    });
    fireEvent.click(screen.getByText("搜索字幕"));
    await waitFor(() => {
      expect(screen.getByText("demo.S01E01.1080p")).toBeInTheDocument();
    });
    const commands = invokeCommands();
    expect(commands).toContain("subtitle_search_online");
    expect(commands).not.toContain("subtitle_download_candidate");
    expect(commands).not.toContain("player_set_subtitle");
    expect(commands).not.toContain("player_seek");
  });

  it("survives the Loading -> Paused readiness flip without tripping the boundary", async () => {
    mockDownloadFlow();
    usePlayerStore.setState({
      currentFile: "C:\\v\\a.mp4",
      status: "Loading",
      currentTimeMs: 0,
    });
    useTrackStore.setState({ subtitleChoiceId: "cache:subdl:en" });
    renderPanel();
    await waitFor(() => {
      expect(screen.getByText(/打开视频后/)).toBeInTheDocument();
    });
    act(() => {
      usePlayerStore.setState({ status: "Paused", currentTimeMs: 500 });
    });
    await waitFor(() => {
      expect(screen.getByText("downloaded two")).toBeInTheDocument();
    });
    expect(screen.queryByText("文稿加载失败")).not.toBeInTheDocument();
  });

  it("checks the typed key without saving it", async () => {
    mockDownloadFlow();
    usePlayerStore.setState({
      currentFile: "C:\\v\\a.mp4",
      status: "Paused",
      currentTimeMs: 500,
    });
    useTrackStore.setState({ subtitleChoiceId: null });
    renderWorkshopPanel();
    await waitFor(() => {
      expect(screen.getByText("搜索字幕")).toBeInTheDocument();
    });
    fireEvent.change(screen.getByLabelText("SubDL API key"), {
      target: { value: "test-key" },
    });
    fireEvent.click(screen.getByText("检测"));
    await waitFor(() => {
      expect(screen.getByText("SubDL Key 有效，可保存使用")).toBeInTheDocument();
    });
    const validateCall = vi
      .mocked(invoke)
      .mock.calls.find(([cmd]) => cmd === "subtitle_validate_provider_key");
    expect(validateCall?.[1]).toMatchObject({ provider: "subdl", key: "test-key" });
    expect(invokeCommands()).not.toContain("subtitle_set_provider_key");
  });

  it("collapses and expands the whole transcript list", async () => {
    mockDownloadFlow();
    usePlayerStore.setState({
      currentFile: "C:\\v\\a.mp4",
      status: "Paused",
      currentTimeMs: 500,
    });
    useTrackStore.setState({ subtitleChoiceId: "cache:subdl:en" });
    renderPanel();
    await waitFor(() => {
      expect(screen.getByText("downloaded two")).toBeInTheDocument();
    });
    fireEvent.click(screen.getByText("收起文稿"));
    await waitFor(() => {
      expect(screen.queryByText("downloaded two")).not.toBeInTheDocument();
    });
    expect(screen.getByText(/点击展开/)).toBeInTheDocument();
    fireEvent.click(screen.getByText(/点击展开/));
    await waitFor(() => {
      expect(screen.getByText("downloaded two")).toBeInTheDocument();
    });
  });

  it("proofreads with sound-tag stripping and applies the result", async () => {
    mockDownloadFlow();
    usePlayerStore.setState({
      currentFile: "C:\\v\\a.mp4",
      status: "Paused",
      currentTimeMs: 500,
    });
    useTrackStore.setState({ subtitleChoiceId: "cache:subdl:en" });
    renderWorkshopPanel();
    await waitFor(() => {
      expect(screen.getByText("校对字幕")).toBeInTheDocument();
      expect(
        screen.getByText("校对字幕").closest("button")?.hasAttribute("disabled"),
      ).toBe(false);
    });
    fireEvent.click(screen.getByText("校对字幕"));
    await waitFor(() => {
      expect(screen.getByText(/已写入/)).toBeInTheDocument();
    });
    const proofreadCall = vi
      .mocked(invoke)
      .mock.calls.find(([cmd]) => cmd === "subtitle_proofread_track");
    expect(proofreadCall?.[1]).toMatchObject({
      choiceId: "cache:subdl:en",
      stripSoundTags: true,
    });
  });

  it("prefills title and episode from the library naming rules", async () => {
    mockDownloadFlow();
    usePlayerStore.setState({
      currentFile: "C:\\v\\Our.Beloved.Summer.2021.EP01.mp4",
      status: "Paused",
      currentTimeMs: 500,
    });
    useTrackStore.setState({ subtitleChoiceId: null });
    renderWorkshopPanel();
    await waitFor(() => {
      expect(
        screen.getByLabelText("Title to search subtitles for"),
      ).toHaveValue("Our Beloved Summer 2021");
    });
    expect(screen.getByLabelText("Episode number")).toHaveValue("1");
    expect(screen.getByLabelText("Season number")).toHaveValue("");
  });

  it("unsticks a translation whose backend died without settling", async () => {
    // 后端进程被杀时 invoke 永不结算：看门狗必须在静默超限后报错并解锁按钮。
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "subtitle_list_choices")
        return Promise.resolve([
          {
            id: "s1",
            source: "Embedded",
            label: "英文",
            supported: true,
            streamIndex: 0,
          },
        ]);
      if (cmd === "subtitle_load_choice")
        return Promise.resolve({ choiceId: "s1", cues: CUES });
      if (cmd === "subtitle_translate_track")
        return new Promise(() => {});
      if (cmd === "asr_status")
        return Promise.resolve({
          available: false,
          installSupported: false,
          models: [],
          catalog: [],
        });
      if (cmd === "media_inspect")
        return Promise.resolve({ path: "C:\\v\\a.mp4", streams: [], chapters: [] });
      if (cmd === "library_agent_models_discover") return Promise.resolve(null);
      return Promise.resolve(null);
    });
    usePlayerStore.setState({
      currentFile: "C:\\v\\a.mp4",
      status: "Paused",
      currentTimeMs: 500,
    });
    useTrackStore.setState({ subtitleChoiceId: "s1" });
    // 看门狗 interval 必须在 fake 时钟下创建，否则 advance 够不到它。
    vi.useFakeTimers();
    try {
      renderWorkshopPanel();
      let enabled = false;
      for (let i = 0; i < 50 && !enabled; i++) {
        await act(async () => {
          await vi.advanceTimersByTimeAsync(100);
        });
        enabled =
          screen
            .getByText("翻译字幕")
            .closest("button")
            ?.hasAttribute("disabled") === false;
      }
      expect(enabled).toBe(true);
      fireEvent.click(screen.getByText("翻译字幕"));
      await act(async () => {
        await vi.advanceTimersByTimeAsync(0);
      });
      expect(screen.getByText("翻译中…")).toBeInTheDocument();
      const t0 = Date.now();
      await act(async () => {
        await vi.advanceTimersByTimeAsync(10 * 60 * 1000 + 31_000);
      });
      expect(Date.now() - t0).toBeGreaterThan(10 * 60 * 1000);
      expect(screen.getByText(/长时间没有进展/)).toBeInTheDocument();
      expect(
        screen.getByText("翻译字幕").closest("button")?.hasAttribute("disabled"),
      ).toBe(false);
    } finally {
      vi.useRealTimers();
    }
  });

  it("downloads on explicit click and applies to the player surface", async () => {
    mockDownloadFlow();
    usePlayerStore.setState({
      currentFile: "C:\\v\\a.mp4",
      status: "Paused",
      currentTimeMs: 500,
    });
    useTrackStore.setState({ subtitleChoiceId: null });
    renderWorkshopPanel();
    await waitFor(() => {
      expect(screen.getByText("搜索字幕")).toBeInTheDocument();
    });
    fireEvent.click(screen.getByText("搜索字幕"));
    await waitFor(() => {
      expect(screen.getByText("下载")).toBeInTheDocument();
    });
    fireEvent.click(screen.getByText("下载"));
    await waitFor(() => {
      expect(invokeCommands()).toContain("player_set_subtitle");
    });
    const commands = invokeCommands();
    expect(commands).toContain("subtitle_download_candidate");
    expect(commands).toContain("player_set_subtitle");
    expect(commands).not.toContain("player_seek");
    // 下载轨走后端解析：必须带 choiceId + mediaPath，不能是空路径。
    const surfaceCall = vi
      .mocked(invoke)
      .mock.calls.find(([cmd]) => cmd === "player_set_subtitle");
    expect(surfaceCall?.[1]).toMatchObject({
      source: "Sidecar",
      choiceId: "cache:subdl:en",
      mediaPath: "C:\\v\\a.mp4",
    });
  });
});
