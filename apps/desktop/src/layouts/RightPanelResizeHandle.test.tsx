import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  DEFAULT_DOCK_WIDTH,
  MAX_DOCK_WIDTH,
  MIN_DOCK_WIDTH,
  useChatUiStore,
} from "@lumina/chat-ui/chatUiStore";

import { RightPanelResizeHandle } from "./RightPanelResizeHandle";

beforeEach(() => {
  useChatUiStore.setState({ dockWidth: DEFAULT_DOCK_WIDTH });
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  localStorage.clear();
});

describe("RightPanelResizeHandle", () => {
  it("exposes the current width as a vertical separator", () => {
    render(<RightPanelResizeHandle />);

    const handle = screen.getByRole("separator", { name: "调整右侧面板宽度" });
    expect(handle).toHaveAttribute("aria-orientation", "vertical");
    expect(handle).toHaveAttribute("aria-valuemin", String(MIN_DOCK_WIDTH));
    expect(handle).toHaveAttribute("aria-valuemax", String(MAX_DOCK_WIDTH));
    expect(handle).toHaveAttribute("aria-valuenow", String(DEFAULT_DOCK_WIDTH));
  });

  it("widens the panel when dragged left and narrows when dragged right", async () => {
    render(<RightPanelResizeHandle />);
    const handle = screen.getByRole("separator", { name: "调整右侧面板宽度" });

    fireEvent.pointerDown(handle, { button: 0, clientX: 500, pointerId: 1 });
    fireEvent.pointerMove(handle, { clientX: 400 });
    fireEvent.pointerUp(handle, { clientX: 400 });

    await waitFor(() => {
      expect(useChatUiStore.getState().dockWidth).toBe(480);
    });

    fireEvent.pointerDown(handle, { button: 0, clientX: 400, pointerId: 1 });
    fireEvent.pointerMove(handle, { clientX: 460 });
    fireEvent.pointerUp(handle, { clientX: 460 });

    await waitFor(() => {
      expect(useChatUiStore.getState().dockWidth).toBe(420);
    });
  });

  it("clamps drags to the min/max bounds", async () => {
    render(<RightPanelResizeHandle />);
    const handle = screen.getByRole("separator", { name: "调整右侧面板宽度" });

    fireEvent.pointerDown(handle, { button: 0, clientX: 500, pointerId: 1 });
    fireEvent.pointerMove(handle, { clientX: -1000 });
    fireEvent.pointerUp(handle, { clientX: -1000 });

    await waitFor(() => {
      expect(useChatUiStore.getState().dockWidth).toBe(MAX_DOCK_WIDTH);
    });

    fireEvent.pointerDown(handle, { button: 0, clientX: 500, pointerId: 1 });
    fireEvent.pointerMove(handle, { clientX: 5000 });
    fireEvent.pointerUp(handle, { clientX: 5000 });

    await waitFor(() => {
      expect(useChatUiStore.getState().dockWidth).toBe(MIN_DOCK_WIDTH);
    });
  });

  it("resets to default on double click and supports arrow keys", () => {
    useChatUiStore.setState({ dockWidth: 500 });
    render(<RightPanelResizeHandle />);
    const handle = screen.getByRole("separator", { name: "调整右侧面板宽度" });

    fireEvent.doubleClick(handle);
    expect(useChatUiStore.getState().dockWidth).toBe(DEFAULT_DOCK_WIDTH);

    fireEvent.keyDown(handle, { key: "ArrowLeft" });
    expect(useChatUiStore.getState().dockWidth).toBe(DEFAULT_DOCK_WIDTH + 12);
    fireEvent.keyDown(handle, { key: "ArrowRight" });
    expect(useChatUiStore.getState().dockWidth).toBe(DEFAULT_DOCK_WIDTH);
  });
});
