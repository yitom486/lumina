import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { FileText } from "lucide-react";

import { TooltipProvider } from "@lumina/ui/tooltip";
import { useChatUiStore } from "@lumina/chat-ui/chatUiStore";

import { WorkspaceRail } from "./WorkspaceRail";

afterEach(() => {
  cleanup();
  useChatUiStore.setState({ chatMounted: false, chatOpen: false });
});

describe("WorkspaceRail", () => {
  it("selects an existing workspace without replacing the panel contract", async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();

    render(
      <TooltipProvider>
        <WorkspaceRail
          items={[{ id: "transcript", label: "文稿", icon: FileText }]}
          activeTab="transcript"
          onSelect={onSelect}
        />
      </TooltipProvider>,
    );

    const transcriptButton = screen.getByRole("button", { name: "文稿" });
    expect(transcriptButton).toHaveAttribute("aria-pressed", "true");

    await user.click(transcriptButton);
    expect(onSelect).toHaveBeenCalledWith("transcript");
  });

  it("closes AI before switching to an ordinary workspace", async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    useChatUiStore.setState({ chatMounted: true, chatOpen: true });

    render(
      <TooltipProvider>
        <WorkspaceRail
          items={[{ id: "transcript", label: "文稿", icon: FileText }]}
          activeTab="transcript"
          onSelect={onSelect}
        />
      </TooltipProvider>,
    );

    await user.click(screen.getByRole("button", { name: "文稿" }));

    expect(useChatUiStore.getState().chatOpen).toBe(false);
    expect(onSelect).toHaveBeenCalledWith("transcript");
  });

  it("uses the same selected, tooltip, and keyboard button contract for AI", async () => {
    const user = userEvent.setup();

    render(
      <TooltipProvider>
        <WorkspaceRail items={[]} activeTab="playlist" onSelect={() => {}} />
      </TooltipProvider>,
    );

    const chatButton = screen.getByRole("button", { name: "打开 AI 对话" });
    expect(chatButton).toHaveAttribute("aria-pressed", "false");

    await user.click(chatButton);

    expect(screen.getByRole("button", { name: "收起 AI 对话" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
  });

  it("exposes the active workspace as the current page and keeps labels visible", () => {
    render(
      <TooltipProvider>
        <WorkspaceRail
          items={[
            { id: "transcript", label: "文稿", icon: FileText },
            { id: "settings", label: "设置", icon: FileText },
          ]}
          activeTab="settings"
          onSelect={() => {}}
        />
      </TooltipProvider>,
    );

    expect(screen.getByRole("button", { name: "设置" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(screen.getByRole("button", { name: "设置" })).toHaveAttribute(
      "data-active",
      "true",
    );
    expect(screen.getByText("设置")).toBeInTheDocument();
  });
});
