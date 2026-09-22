import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { CompanionHeaderPanel } from "./CompanionHeaderPanel";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

function renderPanel() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <div className="sticky top-0">
        <CompanionHeaderPanel onSelectTask={vi.fn()} />
      </div>
      <div className="chat-scroll overflow-y-auto">
        <p>聊天流内容</p>
      </div>
    </QueryClientProvider>,
  );
}

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  localStorage.clear();
});

describe("CompanionHeaderPanel", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("手风琴默认展开，右端为收起按钮，观剧流与快捷操作整体可见", () => {
    renderPanel();

    // 默认展开：右端为 - 收起按钮。
    expect(
      screen.getByRole("button", { name: "收起观剧助手" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("region", { name: "AI 观剧流" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("group", { name: "快捷 AI 操作" }),
    ).toBeInTheDocument();
  });

  it("点右端减号收起为单行，再点加号展开，状态可持久化", () => {
    renderPanel();

    fireEvent.click(screen.getByRole("button", { name: "收起观剧助手" }));

    // 收起后：观剧流与快捷操作都不渲染，只剩单行。
    expect(
      screen.queryByRole("region", { name: "AI 观剧流" }),
    ).not.toBeInTheDocument();
    expect(screen.queryByRole("group", { name: "快捷 AI 操作" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "展开观剧助手" })).toBeInTheDocument();
    expect(localStorage.getItem("lumina-companion-header-expanded-v2")).toBe("0");

    fireEvent.click(screen.getByRole("button", { name: "展开观剧助手" }));
    expect(
      screen.getByRole("region", { name: "AI 观剧流" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("group", { name: "快捷 AI 操作" }),
    ).toBeInTheDocument();
    expect(localStorage.getItem("lumina-companion-header-expanded-v2")).toBe("1");
  });

  it("整行可点击：点标题/副标题文字同样展开/收起", () => {
    renderPanel();

    // 默认展开，点标题文字收起为单行。
    fireEvent.click(screen.getByText("AI 观剧流"));
    expect(
      screen.queryByRole("region", { name: "AI 观剧流" }),
    ).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "展开观剧助手" })).toBeInTheDocument();

    // 点副标题文字重新展开。
    fireEvent.click(screen.getByText("观剧上下文与快捷操作"));
    expect(
      screen.getByRole("region", { name: "AI 观剧流" }),
    ).toBeInTheDocument();
  });

  it("面板在滚动流之外：聊天滚动区内没有观剧流与快捷操作", () => {
    const { container } = renderPanel();

    const scroller = container.querySelector(".chat-scroll");
    expect(scroller).toBeInTheDocument();
    expect(scroller?.textContent).toContain("聊天流内容");
    // 即使默认展开，观剧流与快捷操作也不属于滚动流的一部分。
    expect(scroller?.querySelector('[aria-label="AI 观剧流"]')).toBeNull();
    expect(scroller?.querySelector('[data-companion-header]')).toBeNull();

    const header = container.querySelector("[data-companion-header]");
    expect(header).toBeInTheDocument();
    expect(header?.querySelector('[aria-label="AI 观剧流"]')).toBeInTheDocument();
  });
});
