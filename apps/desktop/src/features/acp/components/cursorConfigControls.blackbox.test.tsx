import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  act,
  cleanup,
  fireEvent,
  render,
  renderHook,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useAcpProfilesStore } from "@lumina/chat-ui/acpProfilesStore";
import type { SessionConfigOption } from "@lumina/chat-ui/types";

import { ChatComposerBar } from "./ChatComposerBar";
import { useCursorConfigControls } from "../useCursorConfigControls";
import type { AcpStatus } from "../types";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  Channel: class {
    onmessage: ((event: unknown) => void) | null = null;
  },
}));

import { invoke } from "@tauri-apps/api/core";

function selectOption(
  id: string,
  values: string[],
  extra: Partial<SessionConfigOption> = {},
): SessionConfigOption {
  return {
    id,
    name: id,
    description: null,
    category: null,
    ...extra,
    kind: {
      kind: "select",
      options: values.map((value) => ({ value, name: value, description: null })),
      current: values[0] ?? null,
    },
  };
}

const CURSOR_EXTRA: SessionConfigOption[] = [
  selectOption("session-mode", ["agent", "plan"], { category: "mode" }),
  selectOption("model", ["grok-4.7", "claude-sonnet-4"]),
  selectOption("reasoning-effort", ["low", "high"]),
  selectOption("context", ["256k", "500k"], { category: "context" }),
  {
    id: "fast",
    name: "fast",
    description: null,
    category: null,
    kind: { kind: "boolean", current: true },
  },
];

function cursorStatus(): AcpStatus {
  return {
    available: true,
    adapterFound: true,
    codexFound: false,
    activeProfileId: "cursor",
    profiles: [],
    message: "ok",
    hint: "",
    responsesOnlyNote: "",
    sessionActive: true,
    busy: false,
    sessionModelOptions: {
      models: [],
      reasoningEfforts: [],
      currentModelId: null,
      currentReasoningEffort: null,
      extraOptions: CURSOR_EXTRA,
    },
  };
}

function renderBar(status: AcpStatus | undefined) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <ChatComposerBar
        value=""
        status={status}
        sessionConnected
        onChange={() => {}}
        onSend={() => {}}
      />
    </QueryClientProvider>,
  );
}

function renderCursorHook(status: AcpStatus | undefined) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return renderHook(
    () =>
      useCursorConfigControls({ status, sessionConnected: true }),
    {
      wrapper: ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={client}>{children}</QueryClientProvider>
      ),
    },
  );
}

beforeEach(() => {
  localStorage.clear();
  useAcpProfilesStore.setState({ activeProfileId: "cursor" });
  vi.mocked(invoke).mockImplementation(() => Promise.resolve(null));
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  localStorage.clear();
});

describe("cursor composer controls blackbox", () => {
  it("renders the five parameterized dimensions instead of the classic dropdowns", () => {
    renderBar(cursorStatus());

    expect(screen.getByLabelText("模式")).toBeInTheDocument();
    expect(screen.getByLabelText("模型")).toBeInTheDocument();
    expect(screen.getByLabelText("思考")).toBeInTheDocument();
    expect(screen.getByLabelText("上下文")).toBeInTheDocument();
    expect(screen.getByLabelText("Fast")).toBeInTheDocument();
    // 经典模型/思考下拉此时是死的，一律隐藏。
    expect(screen.queryByLabelText("思考程度")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("模型 ID")).not.toBeInTheDocument();
  });

  it("falls back to classic controls when cursor advertises no dimensions", () => {
    const status = cursorStatus();
    status.sessionModelOptions = {
      models: [],
      reasoningEfforts: [],
      currentModelId: null,
      currentReasoningEffort: null,
      extraOptions: [],
    };
    renderBar(status);

    expect(screen.queryByLabelText("模式")).not.toBeInTheDocument();
    expect(screen.getByLabelText("模型 ID")).toBeInTheDocument();
  });

  it("sends only advertised values through session/set_config_option", async () => {
    renderBar(cursorStatus());

    fireEvent.change(screen.getByLabelText("模型"), {
      target: { value: "claude-sonnet-4" },
    });
    await waitFor(() => {
      expect(vi.mocked(invoke)).toHaveBeenCalledWith(
        "acp_set_session_config",
        expect.objectContaining({
          configId: "model",
          value: "claude-sonnet-4",
        }),
      );
    });

    fireEvent.click(screen.getByLabelText("Fast"));
    await waitFor(() => {
      expect(vi.mocked(invoke)).toHaveBeenCalledWith(
        "acp_set_session_config",
        expect.objectContaining({ configId: "fast", value: "false" }),
      );
    });
  });

  it("refuses to send unlisted values instead of risking Invalid params", async () => {
    const { result } = renderCursorHook(cursorStatus());

    await act(async () => {
      await result.current.applyConfigOption("model", "grok-4.7[fast=false]");
    });

    expect(
      vi
        .mocked(invoke)
        .mock.calls.filter(([cmd]) => cmd === "acp_set_session_config"),
    ).toHaveLength(0);
    expect(result.current.controlError).toMatch("不在 Agent 下发列表中");
  });

  it("ignores other agents entirely", () => {
    useAcpProfilesStore.setState({ activeProfileId: "codex" });
    renderBar(cursorStatus());

    expect(screen.queryByLabelText("模式")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Fast")).not.toBeInTheDocument();
  });
});
