import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { usePlayerStore } from "@/features/player";
import { useAcpProfilesStore } from "@lumina/chat-ui/acpProfilesStore";
import { useAcpSessionStore } from "@lumina/chat-ui/acpSessionStore";

import { AcpPanel } from "./AcpPanel";

type ChannelHandler = { onmessage: ((event: unknown) => void) | null };

const { channels } = vi.hoisted(() => ({
  channels: [] as ChannelHandler[],
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  Channel: class {
    onmessage: ((event: unknown) => void) | null = null;
    constructor() {
      channels.push(this);
    }
  },
}));

import { invoke } from "@tauri-apps/api/core";

const mockStatus = {
  available: true,
  adapterFound: true,
  codexFound: true,
  activeProfileId: "codex",
  profiles: [],
  message: "ok",
  sessionActive: false,
  busy: false,
};

function renderPanel() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <AcpPanel />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  channels.length = 0;
  localStorage.clear();
  useAcpSessionStore.setState({ savedSession: null });
  useAcpProfilesStore.setState({ activeProfileId: "codex" });
  usePlayerStore.setState({ currentFile: null, status: "Idle" });
  vi.mocked(invoke).mockImplementation((cmd: string) => {
    if (cmd === "acp_status") return Promise.resolve(mockStatus);
    if (cmd === "acp_connect") return Promise.resolve(null);
    // 发送后挂起：回合结束靠 Channel 事件推进，mutation 保持 pending。
    if (cmd === "acp_prompt") return new Promise(() => {});
    return Promise.resolve(null);
  });
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  localStorage.clear();
});

describe("progress ownership blackbox: late events never resurrect stale text", () => {
  it("ignores a progress event arriving after its run finished", async () => {
    renderPanel();
    await waitFor(() => {
      expect(screen.getByText("已连接")).toBeInTheDocument();
    });

    // 真实发送一轮。
    const composer = screen.getByPlaceholderText(/输入问题/);
    fireEvent.change(composer, { target: { value: "请总结一下" } });
    fireEvent.keyDown(composer, { key: "Enter", shiftKey: false });
    await waitFor(() => {
      expect(
        vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "acp_prompt"),
      ).toHaveLength(1);
    });
    const promptChannel = channels[channels.length - 1];
    expect(promptChannel?.onmessage).toBeTypeOf("function");

    // 运行中进度正常显示。
    await act(async () => {
      promptChannel?.onmessage?.({ type: "progress", message: "正在发送问题…" });
    });
    expect(await screen.findByText("正在发送问题…")).toBeInTheDocument();

    // 回答落定：行清掉。
    await act(async () => {
      promptChannel?.onmessage?.({ type: "finished", text: "答完了" });
    });
    await waitFor(() => {
      expect(screen.queryByText("正在发送问题…")).not.toBeInTheDocument();
    });

    // 同一轮的迟到 progress（乱序送达）：不许复活旧文案。
    await act(async () => {
      promptChannel?.onmessage?.({ type: "progress", message: "正在发送问题…" });
    });
    await waitFor(() => {
      expect(screen.queryByText("正在发送问题…")).not.toBeInTheDocument();
    });
    // 气泡内容不受影响。
    expect(screen.getByText("答完了")).toBeInTheDocument();
  });
});
