import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { CompanionHeaderPanel } from "./CompanionHeaderPanel";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

function renderPanel(mode: "watch-feed" | "chat" = "watch-feed") {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <div className="sticky top-0">
        <CompanionHeaderPanel
          mode={mode}
          onModeChange={vi.fn()}
          onSelectTask={vi.fn()}
        />
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

  it("手风琴默认展开，右端为收起按钮，模式与快捷操作整体可见", () => {
    renderPanel();

    // 默认展开：右端为 - 收起按钮。
    expect(
      screen.getByRole("button", { name: "收起观剧助手" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("tablist", { name: "AI 工作区" })).toBeInTheDocument();
    expect(
      screen.getByRole("group", { name: "快捷 AI 操作" }),
    ).toBeInTheDocument();
  });

  it("点右端减号收起为单行，再点加号展开，状态可持久化", () => {
    renderPanel();

    fireEvent.click(screen.getByRole("button", { name: "收起观剧助手" }));

    // 收起后：模式切换与快捷操作都不渲染，只剩单行。
    expect(screen.queryByRole("tablist")).not.toBeInTheDocument();
    expect(screen.queryByRole("group", { name: "快捷 AI 操作" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "展开观剧助手" })).toBeInTheDocument();
    expect(localStorage.getItem("lumina-companion-header-expanded-v2")).toBe("0");

    fireEvent.click(screen.getByRole("button", { name: "展开观剧助手" }));
    expect(screen.getByRole("tablist", { name: "AI 工作区" })).toBeInTheDocument();
    expect(
      screen.getByRole("group", { name: "快捷 AI 操作" }),
    ).toBeInTheDocument();
    expect(localStorage.getItem("lumina-companion-header-expanded-v2")).toBe("1");
  });

  it("面板在滚动流之外：聊天滚动区内没有模式与快捷操作", () => {
    const { container } = renderPanel();

    const scroller = container.querySelector(".chat-scroll");
    expect(scroller).toBeInTheDocument();
    expect(scroller?.textContent).toContain("聊天流内容");
    // 即使默认展开，模式切换与快捷操作也不属于滚动流的一部分。
    expect(scroller?.querySelector('[role="tablist"]')).toBeNull();
    expect(scroller?.querySelector('[data-companion-header]')).toBeNull();

    const header = container.querySelector("[data-companion-header]");
    expect(header).toBeInTheDocument();
    expect(header?.querySelector('[role="tablist"]')).toBeInTheDocument();
  });

  it("自由聊天模式展开时只显示模式切换，不显示观剧流", () => {
    renderPanel("chat");

    expect(screen.getByRole("tablist", { name: "AI 工作区" })).toBeInTheDocument();
    expect(screen.queryByRole("group", { name: "快捷 AI 操作" })).not.toBeInTheDocument();
  });
});
