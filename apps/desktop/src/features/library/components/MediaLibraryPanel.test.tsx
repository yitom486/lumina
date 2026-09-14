import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useLibrarySettingsStore } from "../settingsStore";
import { MediaLibraryPanel } from "./MediaLibraryPanel";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  Channel: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";

const PENDING = [
  {
    root: "D:\\movie",
    group: {
      key: "Our.Beloved.Summer.2021",
      displayName: "Our Beloved Summer 2021",
      kind: "series",
      files: ["Our.Beloved.Summer.2021.EP01.mp4"],
      manualTitle: null,
      resolution: { state: "pending" },
    },
  },
];

const PREVIEW = {
  intent: { title: "Our Beloved Summer", mediaType: "tv" },
  candidates: [{ tmdbId: 135897, mediaType: "tv", title: "那年，我们的夏天", year: 2021 }],
  selection: null,
  canAutoMatch: false,
};

function renderPanel() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <MediaLibraryPanel />
    </QueryClientProvider>,
  );
}

let resolvePreview!: (value: unknown) => void;

beforeEach(() => {
  vi.mocked(invoke).mockImplementation((cmd: string) => {
    if (cmd === "library_status")
      return Promise.resolve({
        running: true,
        roots: ["D:\\movie"],
        pollIntervalSecs: 30,
        lastScanAtMs: null,
        lastScanError: null,
        indexedFiles: 16,
        pendingGroups: 1,
      });
    if (cmd === "library_pending_groups") return Promise.resolve(PENDING);
    if (cmd === "library_groups") return Promise.resolve([]);
    if (cmd === "library_credential_status")
      return Promise.resolve({ modelApiKeySaved: false, tmdbAccessTokenSaved: true });
    if (cmd === "library_resolve_preview")
      return new Promise((resolve) => {
        resolvePreview = resolve as (value: unknown) => void;
      });
    if (cmd === "library_search_tmdb")
      return Promise.resolve([
        { tmdbId: 135897, mediaType: "tv", title: "那年，我们的夏天", year: 2021 },
      ]);
    if (cmd === "library_apply_tmdb_match")
      return Promise.resolve({ root: "D:\\movie", groupKey: "g", tmdbId: 1, mediaType: "tv", writtenFiles: [] });
    return Promise.resolve(null);
  });
  useLibrarySettingsStore.setState({
    resolverProvider: "acpAgent",
    agentModelId: "test-model",
    privacyAcknowledged: true,
  });
  localStorage.clear();
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  localStorage.clear();
});

describe("MediaLibraryPanel pending groups", () => {
  it("prefills the title input with the known group title", async () => {
    renderPanel();
    const input = (await screen.findByPlaceholderText(
      "输入作品名后查 TMDb",
    )) as HTMLInputElement;
    expect(input.value).toBe("Our Beloved Summer 2021");
  });

  it("searches TMDb manually and applies with chosen fields", async () => {
    renderPanel();
    fireEvent.click(await screen.findByText("查 TMDb"));
    const confirm = await screen.findByText("确认拉取");
    fireEvent.click(screen.getByLabelText("演员阵容"));
    fireEvent.click(confirm);
    await waitFor(() => {
      const call = vi
        .mocked(invoke)
        .mock.calls.find(([cmd]) => cmd === "library_apply_tmdb_match");
      expect(call?.[1]).toMatchObject({
        tmdbId: 135897,
        mediaType: "tv",
        fields: { basic: true, cast: false, episodes: true },
      });
    });
  });

  it("shows busy state while recognizing and offers one-click confirm", async () => {
    renderPanel();
    const identify = await screen.findByText("智能识别");
    fireEvent.click(identify);
    await waitFor(() => {
      expect(screen.getByText("识别中…")).toBeInTheDocument();
    });
    expect(screen.getByText("识别中…").closest("button")).toBeDisabled();
    resolvePreview(PREVIEW);
    await waitFor(() => {
      expect(screen.getByText(/确认首选：那年，我们的夏天/)).toBeInTheDocument();
    });
  });

  it("confirms the top candidate with progress feedback", async () => {
    renderPanel();
    fireEvent.click(await screen.findByText("智能识别"));
    resolvePreview(PREVIEW);
    const confirm = await screen.findByText(/确认首选：那年，我们的夏天/);
    fireEvent.click(confirm);
    await waitFor(() => {
      expect(
        vi.mocked(invoke).mock.calls.some(([cmd]) => cmd === "library_apply_tmdb_match"),
      ).toBe(true);
    });
  });
});
