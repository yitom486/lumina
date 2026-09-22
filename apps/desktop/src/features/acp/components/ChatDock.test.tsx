import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import { useChatUiStore } from "@lumina/chat-ui/chatUiStore";

import { ChatDock } from "./ChatDock";

vi.mock("@/components/PanelErrorBoundary", () => ({
  PanelErrorBoundary: ({ children }: { children: ReactNode }) => (
    <>{children}</>
  ),
}));

vi.mock("./AcpPanel", () => ({
  AcpPanel: () => (
    <>
      <button type="button">内部控件</button>
      <input aria-label="AI 对话输入框" />
    </>
  ),
}));

afterEach(() => {
  cleanup();
  useChatUiStore.setState({ chatMounted: false, chatOpen: false });
  vi.restoreAllMocks();
});

describe("ChatDock", () => {
  it("keeps the open dock beside the native content sibling", () => {
    useChatUiStore.setState({ chatMounted: true, chatOpen: true });

    render(
      <main>
        <section aria-label="Native video surface" />
        <ChatDock />
      </main>,
    );

    const main = screen.getByRole("main");
    const dock = screen.getByRole("complementary");
    expect(main.children).toHaveLength(2);
    expect(main.children[0]).toHaveAttribute("aria-label", "Native video surface");
    expect(dock).toHaveAttribute("aria-hidden", "false");
    expect(dock).not.toHaveClass("pointer-events-none");
    expect(
      screen.getByRole("textbox", { name: "AI 对话输入框", hidden: true }),
    ).toBeInTheDocument();
  });

  it("renders without a header bar and labels the dock region", () => {
    useChatUiStore.setState({ chatMounted: true, chatOpen: true });
    render(<ChatDock />);

    const dock = screen.getByRole("complementary", { name: "AI 对话" });
    expect(dock).toBeInTheDocument();
    // 标题栏已端掉：没有多余的标题行，只剩内容。
    expect(screen.queryByRole("button", { name: "收起对话" })).not.toBeInTheDocument();
  });

  it("hides the dock without unmounting its private UI state", () => {
    useChatUiStore.setState({ chatMounted: true, chatOpen: true });
    render(<ChatDock />);

    const input = screen.getByRole("textbox", { name: "AI 对话输入框" });
    // 标题栏已无 X：收起只走导航栏开关/Esc（同一 closeChat 动作）。
    act(() => {
      useChatUiStore.getState().closeChat();
    });

    const dock = screen.getByRole("complementary", { hidden: true });
    expect(dock).toHaveAttribute("aria-hidden", "true");
    expect(dock).toHaveClass("w-0", "pointer-events-none");
    expect(
      screen.queryByRole("button", { name: "收起对话" }),
    ).not.toBeInTheDocument();
    expect(input).toBeInTheDocument();
  });

  it("keeps the mounted dock inert while closed and restores focusability when opened", async () => {
    const user = userEvent.setup();
    useChatUiStore.setState({ chatMounted: true, chatOpen: false });

    render(<ChatDock />);

    const dock = screen.getByRole("complementary", { hidden: true });
    expect(dock).toHaveAttribute("aria-hidden", "true");
    expect(dock).toHaveAttribute("inert");
    expect(
      screen.getByRole("button", { name: "内部控件", hidden: true }),
    ).toBeInTheDocument();

    await user.keyboard("{Escape}");
    expect(useChatUiStore.getState().chatOpen).toBe(false);

    useChatUiStore.getState().openChat();

    await waitFor(() => {
      expect(dock).toHaveAttribute("aria-hidden", "false");
      expect(dock).not.toHaveAttribute("inert");
      expect(
        screen.getByRole("button", { name: "内部控件" }),
      ).toBeEnabled();
    });
  });

  it("closes from Escape while open without unmounting the dock", async () => {
    const user = userEvent.setup();
    useChatUiStore.setState({ chatMounted: true, chatOpen: true });

    render(<ChatDock />);
    const dock = screen.getByRole("complementary");

    await user.keyboard("{Escape}");

    expect(useChatUiStore.getState().chatOpen).toBe(false);
    expect(screen.getByRole("complementary", { hidden: true })).toBe(dock);
    expect(dock).toHaveAttribute("inert");
  });
});
