import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import { CompanionQuickActions } from "./CompanionQuickActions";

afterEach(cleanup);

describe("CompanionQuickActions", () => {
  it("renders stable task shortcuts as accessible buttons", () => {
    render(<CompanionQuickActions onSelectTask={vi.fn()} />);

    expect(screen.getByRole("group", { name: "快捷 AI 操作" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "本段总结" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "后续看点" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "观众问题" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "剧情梳理" })).toBeInTheDocument();
  });

  it("emits only the selected stable task id, including from keyboard input", async () => {
    const user = userEvent.setup();
    const onSelectTask = vi.fn();
    render(<CompanionQuickActions onSelectTask={onSelectTask} />);

    const recap = screen.getByRole("button", { name: "本段总结" });
    await user.click(recap);
    expect(onSelectTask).toHaveBeenLastCalledWith("chapter_recap");

    const outlook = screen.getByRole("button", { name: "后续看点" });
    outlook.focus();
    await user.keyboard("{Enter}");
    expect(onSelectTask).toHaveBeenLastCalledWith("chapter_outlook");
  });

  it("disables every shortcut without invoking the callback", async () => {
    const user = userEvent.setup();
    const onSelectTask = vi.fn();
    render(<CompanionQuickActions disabled onSelectTask={onSelectTask} />);

    const buttons = screen.getAllByRole("button");
    expect(buttons).toHaveLength(4);
    for (const button of buttons) {
      expect(button).toBeDisabled();
      await user.click(button);
    }
    expect(onSelectTask).not.toHaveBeenCalled();
  });
});
