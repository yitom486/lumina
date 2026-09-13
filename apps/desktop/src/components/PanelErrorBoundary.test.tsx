import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import { PanelErrorBoundary } from "./PanelErrorBoundary";

function BrokenChild(): never {
  throw new Error("boom");
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("PanelErrorBoundary", () => {
  it("shows fallback instead of crashing the tree", () => {
    const spy = vi.spyOn(console, "error").mockImplementation(() => {});

    render(
      <PanelErrorBoundary panelLabel="测试面板" scope="test">
        <BrokenChild />
      </PanelErrorBoundary>,
    );

    expect(screen.getByText("测试面板加载失败")).toBeInTheDocument();
    expect(screen.getByText(/该功能暂时不可用/)).toBeInTheDocument();
    expect(screen.queryByText("boom")).not.toBeInTheDocument();
    expect(spy).toHaveBeenCalled();
  });

  it("retries after clicking 重试", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    let shouldThrow = true;

    function MaybeBroken() {
      if (shouldThrow) throw new Error("once");
      return <p>恢复成功</p>;
    }

    render(
      <PanelErrorBoundary panelLabel="面板" scope="test-retry">
        <MaybeBroken />
      </PanelErrorBoundary>,
    );

    expect(screen.getByText("面板加载失败")).toBeInTheDocument();
    shouldThrow = false;
    await userEvent.click(screen.getByRole("button", { name: "重试" }));
    expect(screen.getByText("恢复成功")).toBeInTheDocument();
  });

  it("clears error when resetKey changes", () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    let shouldThrow = true;

    function MaybeBroken() {
      if (shouldThrow) throw new Error("once");
      return <p>切换后恢复</p>;
    }

    const { rerender } = render(
      <PanelErrorBoundary panelLabel="面板" scope="test-key" resetKey="a">
        <MaybeBroken />
      </PanelErrorBoundary>,
    );

    expect(screen.getByText("面板加载失败")).toBeInTheDocument();
    shouldThrow = false;
    rerender(
      <PanelErrorBoundary panelLabel="面板" scope="test-key" resetKey="b">
        <MaybeBroken />
      </PanelErrorBoundary>,
    );
    expect(screen.getByText("切换后恢复")).toBeInTheDocument();
  });
});
