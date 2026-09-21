import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { usePlayerStore } from "../store";
import { SeekBar } from "./SeekBar";

vi.mock("@/features/media", () => ({
  useMediaInfoQuery: () => ({ data: null }),
}));

afterEach(() => {
  usePlayerStore.setState({
    currentFile: null,
    currentTimeMs: 0,
    durationMs: 0,
    durationHintMs: null,
    status: "Idle",
  });
});

describe("SeekBar", () => {
  it("renders the hover preview below the HTML seek track", () => {
    usePlayerStore.setState({
      currentTimeMs: 10_000,
      durationMs: 60_000,
      status: "Ready",
    });

    render(<SeekBar />);
    const slider = screen.getByRole("slider");
    const track = slider.closest("div.relative");
    expect(track).not.toBeNull();
    vi.spyOn(track as HTMLElement, "getBoundingClientRect").mockReturnValue({
      left: 0,
      width: 600,
      top: 0,
      right: 600,
      bottom: 8,
      height: 8,
      x: 0,
      y: 0,
      toJSON: () => ({}),
    });

    fireEvent.pointerMove(track as HTMLElement, {
      clientX: 300,
      pointerType: "mouse",
    });

    const preview = screen.getByRole("tooltip");
    expect(preview).toHaveAttribute("data-seek-preview-placement", "below");
    expect(preview.className).toContain("top-full");
    expect(preview.className).not.toContain("bottom-full");
  });
});
