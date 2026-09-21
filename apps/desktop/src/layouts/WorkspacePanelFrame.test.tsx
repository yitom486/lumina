import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { WorkspacePanelFrame } from "./WorkspacePanelFrame";

afterEach(() => cleanup());

describe("WorkspacePanelFrame", () => {
  it("provides the shared panel shell for workspace content and actions", () => {
    render(
      <WorkspacePanelFrame
        title="文稿"
        subtitle="与播放器并行的阅读面板"
        actions={<button type="button">关闭</button>}
      >
        <p>章节内容</p>
      </WorkspacePanelFrame>,
    );

    expect(screen.getByRole("region", { name: "文稿" })).toBeInTheDocument();
    expect(screen.getByText("与播放器并行的阅读面板")).toBeInTheDocument();
    expect(screen.getByText("章节内容")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "关闭" })).toBeInTheDocument();
  });
});
