import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { usePlayerStore } from "@/features/player";

import type { AssistantAction } from "@lumina/chat-ui/assistantBlocks";
import type { ChatTurn } from "../types";
import { ChatTurnView } from "./ChatTurnView";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";

function makeTurn(partial: Partial<ChatTurn> & Pick<ChatTurn, "id">): ChatTurn {
  return {
    userText: "剧情梳理",
    answer: "",
    status: "done",
    activities: [],
    showActivities: false,
    ...partial,
  };
}

function renderTurn(turn: ChatTurn, onAssistantAction?: (action: AssistantAction) => void) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <ChatTurnView turn={turn} onAssistantAction={onAssistantAction} />
    </QueryClientProvider>,
  );
}

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  localStorage.clear();
});

describe("structured result citation blackbox: ref -> jumpable", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "media_inspect")
        return Promise.resolve({
          path: "C:\\v\\a.mp4",
          durationMs: 3_600_000,
          streams: [],
          chapters: [],
        });
      if (cmd === "player_seek")
        return Promise.resolve({ status: "Paused", currentTimeMs: 698_000 });
      return Promise.resolve(null);
    });
    usePlayerStore.setState({
      currentFile: "C:\\v\\a.mp4",
      status: "Paused",
      currentTimeMs: 0,
      durationMs: 3_600_000,
    });
  });

  it("renders uploaded [11:38]-[11:59] ref as a seek button through the full chain", async () => {
    const onAssistantAction = vi.fn();
    renderTurn(
      makeTurn({
        id: "blackbox-structured-ref",
        answer: JSON.stringify({
          contract: "chapter_recap.v1",
          chapter: { title: "1792个夏日", position: "12:09" },
          spoiler_boundary: "current_position",
          recap: "崔雄与国延秀重新面对过去的关系。",
          evidence: [{ ref: "[11:38]-[11:59]", fact: "两人讨论未来选择。" }],
          uncertainty: ["当前台词窗口并不完整。"],
        }),
        status: "done",
      }),
      onAssistantAction,
    );

    // 完整链路：JSON contract -> structured-result -> renderMarkdown ->
    // ChatMarkdown -> EvidenceCitation。验证通过后必须是 button。
    const cite = await screen.findByRole("button", { name: "[11:38]-[11:59]" });
    expect(cite).toBeInTheDocument();
    expect(cite).toHaveAttribute("title", expect.stringContaining("跳转到"));

    // 内联跳转：点击直接 seek 到 range 起点 11:38 = 698_000ms。
    fireEvent.click(cite);
    await waitFor(() => {
      const seeks = vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "player_seek");
      expect(seeks).toHaveLength(1);
    });

    // 结构化结果自动派生的 seek chips 必须同时存在且可回调。
    const chip1138 = await screen.findByRole("button", { name: /跳转到 11:38/ });
    const chip1159 = await screen.findByRole("button", { name: /跳转到 11:59/ });
    expect(chip1138).not.toBeDisabled();
    expect(chip1159).not.toBeDisabled();
    fireEvent.click(chip1138);
    expect(onAssistantAction).toHaveBeenCalledWith({
      type: "seek",
      anchor: { startMs: 698_000 },
    });
    fireEvent.click(chip1159);
    expect(onAssistantAction).toHaveBeenCalledWith({
      type: "seek",
      anchor: { startMs: 719_000 },
    });
  });

  it("renders watch-feed-card bullets with the same jumpable ref", async () => {
    renderTurn(
      makeTurn({
        id: "blackbox-watchfeed-ref",
        answer: JSON.stringify({
          blocks: [
            {
              kind: "watch-feed-card",
              id: "feed-1",
              title: "剧情梳理",
              summary: "主线推进。",
              bullets: ["关键转折。 · [12:32]"],
              spoilerLevel: "current",
              actions: [],
            },
          ],
        }),
        status: "done",
      }),
    );

    const cite = await screen.findByRole("button", { name: "[12:32]" });
    expect(cite).toBeInTheDocument();
    fireEvent.click(cite);
    await waitFor(() => {
      const seeks = vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "player_seek");
      expect(seeks).toHaveLength(1);
    });
  });

  it("keeps out-of-range refs non-jumpable and never seeks", async () => {
    renderTurn(
      makeTurn({
        id: "blackbox-out-of-range",
        answer: JSON.stringify({
          contract: "plot_summary.v1",
          summary: "主线推进。",
          evidence: [{ ref: "[99:59:59]", fact: "伪造的超长引用。" }],
        }),
        status: "done",
      }),
    );

    await waitFor(() => {
      expect(screen.getByText("[99:59:59]")).toBeInTheDocument();
    });
    expect(screen.queryByRole("button", { name: "[99:59:59]" })).not.toBeInTheDocument();
    expect(
      vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "player_seek"),
    ).toHaveLength(0);
  });
});
