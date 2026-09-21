import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { CompanionModeTabs } from "@lumina/chat-ui/components/CompanionModeTabs";

afterEach(cleanup);

describe("CompanionModeTabs", () => {
  it("exposes both companion projections as accessible tabs", () => {
    render(<CompanionModeTabs value="watch-feed" onChange={vi.fn()} />);

    expect(screen.getByRole("tab", { name: /AI 观剧流/ })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(screen.getByRole("tab", { name: /自由聊天/ })).toHaveAttribute(
      "aria-selected",
      "false",
    );
  });

  it("reports projection changes without creating a session", () => {
    const onChange = vi.fn();
    render(<CompanionModeTabs value="watch-feed" onChange={onChange} />);

    fireEvent.click(screen.getByRole("tab", { name: /自由聊天/ }));

    expect(onChange).toHaveBeenCalledWith("chat");
  });
});
