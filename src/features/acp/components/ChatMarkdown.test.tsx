import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { ChatMarkdown } from "./ChatMarkdown";

afterEach(() => {
  cleanup();
});

describe("ChatMarkdown", () => {
  it("renders headings and inline code", () => {
    render(<ChatMarkdown content={"## 标题\n\n这是 `code` 示例。"} />);
    expect(screen.getByRole("heading", { level: 2, name: "标题" })).toBeInTheDocument();
    expect(screen.getByText("code")).toBeInTheDocument();
  });

  it("renders block math with KaTeX", () => {
    const { container } = render(
      <ChatMarkdown content={"$$E = mc^2$$"} />,
    );
    expect(container.querySelector(".katex")).toBeTruthy();
  });
});
