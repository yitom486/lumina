import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useChatUiStore } from "@lumina/chat-ui/chatUiStore";
import { TooltipProvider } from "@lumina/ui/tooltip";

import { AppShell } from "./AppShell";

vi.mock("@tauri-apps/api/app", () => ({
  getVersion: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: vi.fn(),
  revealItemInDir: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { getVersion } from "@tauri-apps/api/app";
import { openUrl } from "@tauri-apps/plugin-opener";

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

beforeEach(() => {
  useChatUiStore.setState({
    chatMounted: false,
    chatOpen: false,
    acpResponding: false,
  });
});

describe("AboutButton", () => {
  it("shows the runtime version in the about dialog", async () => {
    vi.mocked(getVersion).mockResolvedValue("0.3.0");
    const user = userEvent.setup();
    render(
      <TooltipProvider>
        <AppShell>
          <div />
        </AppShell>
      </TooltipProvider>,
    );

    await user.click(screen.getByRole("button", { name: "关于 Lumina" }));
    await waitFor(() => {
      expect(screen.getByText("版本 0.3.0")).toBeInTheDocument();
    });
  });

  it("opens the release page from the about dialog", async () => {
    vi.mocked(getVersion).mockResolvedValue("0.3.0");
    vi.mocked(openUrl).mockResolvedValue(undefined);
    const user = userEvent.setup();
    render(
      <TooltipProvider>
        <AppShell>
          <div />
        </AppShell>
      </TooltipProvider>,
    );

    await user.click(screen.getByRole("button", { name: "关于 Lumina" }));
    await user.click(await screen.findByRole("button", { name: "下载更新" }));
    expect(openUrl).toHaveBeenCalledWith(
      "https://github.com/yitom486/lumina-app/releases",
    );
  });
});

describe("AppShell AI entry", () => {
  it("toggles the AI sibling entry without changing the ordinary workspace content", async () => {
    const user = userEvent.setup();
    render(
      <TooltipProvider>
        <AppShell>
          <div data-testid="ordinary-workspace" />
        </AppShell>
      </TooltipProvider>,
    );

    expect(screen.getByTestId("ordinary-workspace")).toBeInTheDocument();
    const toggle = screen.getByRole("button", { name: "打开 AI 对话" });
    await user.click(toggle);
    expect(toggle).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByTestId("ordinary-workspace")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "收起 AI 对话" }));
    expect(screen.getByRole("button", { name: "打开 AI 对话" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
  });
});

it.todo("switches between WorkspacePanelFrame and ChatDock siblings with inert focus isolation");
