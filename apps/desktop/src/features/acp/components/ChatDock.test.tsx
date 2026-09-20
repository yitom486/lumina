import { cleanup, render, screen, waitFor } from "@testing-library/react";
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

vi.mock("@/layouts/WorkspacePanelFrame", () => ({
  WorkspacePanelFrame: ({ children, actions }: { children: ReactNode; actions?: ReactNode }) => (
    <section>
      {actions}
      {children}
    </section>
  ),
}));

vi.mock("./AcpPanel", () => ({
  AcpPanel: () => <button type="button">内部控件</button>,
}));

afterEach(() => {
  cleanup();
  useChatUiStore.setState({ chatMounted: false, chatOpen: false });
  vi.restoreAllMocks();
});

describe("ChatDock", () => {
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
      expect(screen.getByRole("button", { name: "收起对话" })).toBeEnabled();
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
