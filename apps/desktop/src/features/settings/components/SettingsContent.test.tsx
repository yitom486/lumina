import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("@/features/library", () => ({
  LibrarySettingsPanel: () => <div>真实影视库设置入口</div>,
}));
vi.mock("@/features/transcript", () => ({
  SubtitleWorkshopPanel: () => <div>真实字幕工作坊入口</div>,
}));
vi.mock("@/features/ytdl", () => ({
  OnlineResourceSettingsPanel: () => <div>真实在线资源入口</div>,
}));
vi.mock("./AiAutomationSettingsPanel", () => ({
  AiAutomationSettingsPanel: () => <div>真实 AI 与自动化入口</div>,
}));
vi.mock("./PlaybackInterfaceSettingsPanel", () => ({
  PlaybackInterfaceSettingsPanel: () => <div>真实播放与界面入口</div>,
}));

import { SettingsContent } from "./SettingsContent";

describe("SettingsContent", () => {
  afterEach(() => {
    cleanup();
  });

  it.each([
    ["playback", "真实播放与界面入口", "真实字幕工作坊入口"],
    ["subtitle", "真实字幕工作坊入口", "真实影视库设置入口"],
    ["library", "真实影视库设置入口", "真实在线资源入口"],
    ["online", "真实在线资源入口", "真实 AI 与自动化入口"],
    ["automation", "真实 AI 与自动化入口", "真实播放与界面入口"],
  ] as const)(
    "renders only the active %s settings panel",
    (category, activeMarker, inactiveMarker) => {
      render(<SettingsContent category={category} />);

      expect(screen.getByText(activeMarker)).toBeInTheDocument();
      expect(screen.queryByText(inactiveMarker)).not.toBeInTheDocument();
    },
  );

  it("defaults to the playback category for standalone callers", () => {
    render(<SettingsContent />);

    expect(screen.getByText("真实播放与界面入口")).toBeInTheDocument();
    expect(screen.queryByText("真实影视库设置入口")).not.toBeInTheDocument();
  });
});
