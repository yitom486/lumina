import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useChatUiStore } from "@lumina/chat-ui/chatUiStore";

import { ChatDock } from "./ChatDock";

vi.mock("./AcpPanel", () => ({
  AcpPanel: () => <input aria-label="AI 对话输入框" />,
}));

beforeEach(() => {
  useChatUiStore.setState({
    chatMounted: false,
    chatOpen: false,
    acpResponding: false,
  });
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("ChatDock layout sibling", () => {
  it("does not mount the AI sibling before the user opens it", () => {
    render(<ChatDock />);

    expect(screen.queryByRole("complementary")).not.toBeInTheDocument();
  });

  it("keeps the open dock beside the content with an accessible live region", () => {
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

  it("hides the dock as a sibling without unmounting its private UI state", () => {
    useChatUiStore.setState({ chatMounted: true, chatOpen: true });
    render(<ChatDock />);

    const input = screen.getByRole("textbox", { name: "AI 对话输入框" });
    fireEvent.click(screen.getByRole("button", { name: "收起对话" }));

    const dock = screen.getByRole("complementary", { hidden: true });
    expect(dock).toHaveAttribute("aria-hidden", "true");
    expect(dock).toHaveClass("w-0", "pointer-events-none");
    expect(screen.queryByRole("button", { name: "收起对话" })).not.toBeInTheDocument();
    expect(input).toBeInTheDocument();
  });

  it("closes from Escape while leaving the mounted dock recoverable", () => {
    useChatUiStore.setState({ chatMounted: true, chatOpen: true });
    render(<ChatDock />);

    fireEvent.keyDown(window, { key: "Escape" });

    expect(useChatUiStore.getState().chatOpen).toBe(false);
    expect(
      screen.getByRole("complementary", { hidden: true }),
    ).toHaveAttribute("aria-hidden", "true");
    expect(
      screen.getByRole("textbox", { name: "AI 对话输入框", hidden: true }),
    ).toBeInTheDocument();
  });
});

it.todo("uses native inert and returns focus to the visible sibling when the dock closes");

describe.todo("AI 观剧流 / 自由聊天 tabs", () => {
  it.todo("isolates drafts, activities, errors, and session state when switching tabs");
});
