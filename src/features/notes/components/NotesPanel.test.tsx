import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { usePlayerStore } from "@/features/player";

import { NotesPanel } from "./NotesPanel";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";

const FRAMED_NOTE = {
  id: "n1",
  mediaPath: "C:\\v\\a.mp4",
  positionMs: 5000,
  body: "带图批注",
  quotes: [],
  frames: [{ atMs: 5000, file: "n1.jpg" }],
  createdAt: "t",
  updatedAt: "t",
};

function renderPanel() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <NotesPanel />
    </QueryClientProvider>,
  );
}

function seekCalls() {
  return vi
    .mocked(invoke)
    .mock.calls.filter(([cmd]) => cmd === "player_seek");
}

beforeEach(() => {
  vi.mocked(invoke).mockImplementation((cmd: string) => {
    if (cmd === "notes_list") return Promise.resolve([FRAMED_NOTE]);
    if (cmd === "notes_get_frame")
      return Promise.resolve({ mime: "image/jpeg", data: "AAA" });
    if (cmd === "notes_create")
      return Promise.resolve({ ...FRAMED_NOTE, id: "n2", frames: [] });
    if (cmd === "player_seek")
      return Promise.resolve({ status: "Paused", currentTimeMs: 5000 });
    return Promise.resolve(null);
  });
  usePlayerStore.setState({
    currentFile: "C:\\v\\a.mp4",
    status: "Paused",
    currentTimeMs: 9000,
  });
  localStorage.clear();
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  localStorage.clear();
});

describe("NotesPanel frame references", () => {
  it("renders the thumbnail and seeks to the frame moment on click", async () => {
    renderPanel();
    const img = (await screen.findByAltText(
      "批注画面 0:05",
    )) as HTMLImageElement;
    expect(img.src).toContain("data:image/jpeg;base64,AAA");
    const button = img.closest("button");
    expect(button).not.toBeNull();
    fireEvent.click(button as Element);
    await waitFor(() => {
      expect(seekCalls()).toHaveLength(1);
    });
  });

  it("passes includeFrame when the checkbox is on", async () => {
    renderPanel();
    await waitFor(() => {
      expect(screen.getByText("带图批注")).toBeInTheDocument();
    });
    fireEvent.change(screen.getByPlaceholderText("写下这一刻的感想…"), {
      target: { value: "新批注" },
    });
    fireEvent.click(screen.getByText("附带当前画面"));
    fireEvent.click(screen.getByText("保存批注"));
    await waitFor(() => {
      const create = vi
        .mocked(invoke)
        .mock.calls.find(([cmd]) => cmd === "notes_create");
      expect(create?.[1]).toMatchObject({
        input: expect.objectContaining({ includeFrame: true }),
      });
    });
  });

  it("does not fetch frames for frameless notes", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "notes_list")
        return Promise.resolve([{ ...FRAMED_NOTE, id: "n9", frames: [] }]);
      return Promise.resolve(null);
    });
    renderPanel();
    await waitFor(() => {
      expect(screen.getByText("带图批注")).toBeInTheDocument();
    });
    expect(
      vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "notes_get_frame"),
    ).toHaveLength(0);
  });
});
