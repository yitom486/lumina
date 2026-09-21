import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { usePlayerStore } from "@/features/player";

import { OnlineResourceSettingsPanel } from "./OnlineResourceSettingsPanel";
import { OnlineSourcePanel } from "./OnlineSourcePanel";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(null),
}));

function renderWithQuery(ui: ReactNode) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(<QueryClientProvider client={client}>{ui}</QueryClientProvider>);
}

afterEach(() => {
  cleanup();
  usePlayerStore.setState({ currentFile: null, sourceKind: "local", status: "Idle" });
});

describe("OnlineSourcePanel view separation", () => {
  it("keeps URL entry and resolver operations in settings", () => {
    renderWithQuery(<OnlineSourcePanel />);
    expect(screen.queryByPlaceholderText("https://www.youtube.com/watch?v=…")).not.toBeInTheDocument();
    expect(screen.getByText("尚未打开在线媒体；请在设置的「在线资源」中输入 URL。")).toBeInTheDocument();

    cleanup();
    renderWithQuery(<OnlineResourceSettingsPanel />);
    expect(screen.getByPlaceholderText("https://www.youtube.com/watch?v=…")).toBeInTheDocument();
    expect(screen.getByText("在线解析 / 登录态")).toBeInTheDocument();
  });
});
