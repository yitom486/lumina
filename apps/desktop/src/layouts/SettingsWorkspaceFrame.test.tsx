import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import {
  SETTINGS_CATEGORIES,
  SettingsWorkspaceFrame,
  type SettingsCategoryId,
} from "./SettingsWorkspaceFrame";

afterEach(() => cleanup());

describe("SettingsWorkspaceFrame", () => {
  it("provides accessible category navigation and a children content slot", async () => {
    const user = userEvent.setup();
    const onCategoryChange = vi.fn();

    render(
      <SettingsWorkspaceFrame
        activeCategory="automation"
        onCategoryChange={onCategoryChange}
      >
        <div data-testid="settings-content">真实设置内容</div>
      </SettingsWorkspaceFrame>,
    );

    expect(SETTINGS_CATEGORIES).toHaveLength(5);
    expect(screen.getByRole("region", { name: "设置工作区" })).toBeInTheDocument();
    expect(screen.getByRole("navigation", { name: "设置分类" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "AI 与自动化" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(screen.getByTestId("settings-content")).toHaveTextContent("真实设置内容");

    await user.click(screen.getByRole("button", { name: "字幕工作坊" }));
    expect(onCategoryChange).toHaveBeenCalledWith("subtitle");
  });

  it("renders feature content through the active-category callback", () => {
    const renderContent = vi.fn((category: SettingsCategoryId) => (
      <p data-testid="rendered-settings-content">当前：{category}</p>
    ));

    render(
      <SettingsWorkspaceFrame
        activeCategory="playback"
        onCategoryChange={() => {}}
        renderContent={renderContent}
      />,
    );

    expect(renderContent).toHaveBeenCalledWith("playback");
    expect(screen.getByTestId("rendered-settings-content")).toHaveTextContent(
      "当前：playback",
    );
  });
});
