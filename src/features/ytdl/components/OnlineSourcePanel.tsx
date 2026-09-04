/** Online URL open + cookie settings — lives in sidebar (never over HWND). */

import { useState } from "react";
import { useQuery } from "@tanstack/react-query";

import { Button } from "@/components/ui/button";
import { OnlineSourceSettings } from "@/features/ytdl/components/OnlineSourceSettings";
import { usePlayerStore } from "@/features/player";
import { getCachedYtdlResolve } from "../api";

export function OnlineSourcePanel() {
  const busy = usePlayerStore((s) => s.busy);
  const openUrl = usePlayerStore((s) => s.openUrl);
  const statusMessage = usePlayerStore((s) => s.statusMessage);
  const error = usePlayerStore((s) => s.error);
  const status = usePlayerStore((s) => s.status);
  const currentFile = usePlayerStore((s) => s.currentFile);
  const sourceKind = usePlayerStore((s) => s.sourceKind);
  const [url, setUrl] = useState("");

  const canSubmit =
    /^https?:\/\//i.test(url.trim()) && url.trim().length > 10;

  const submit = async () => {
    if (!canSubmit || busy) return;
    const submitted = url.trim();
    await openUrl(submitted);
    // Keep URL on failure so the user can retry after installing / cookies.
    if (usePlayerStore.getState().status !== "Error") {
      setUrl("");
    }
  };

  const openFailed = status === "Error" && Boolean(error?.message);
  const playingRemote =
    sourceKind === "remote" && Boolean(currentFile) && status !== "Error";
  const mediaQuery = useQuery({
    queryKey: ["ytdl-resolve", currentFile],
    queryFn: () => getCachedYtdlResolve(currentFile as string),
    enabled: playingRemote,
    staleTime: Infinity,
  });

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-y-auto p-3">
      <div className="space-y-2">
        <h2 className="text-sm font-medium">打开在线视频</h2>
        <p className="text-xs text-muted-foreground">
          粘贴完整链接后点「打开链接」。公开视频通常可直接播；若提示需要登录，请用下方「导入
          cookies.txt」（不是 Google 一键授权）。
        </p>
        <input
          type="url"
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          placeholder="https://www.youtube.com/watch?v=…"
          className="w-full rounded-md border border-border bg-background px-3 py-2 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
          disabled={busy}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              void submit();
            }
          }}
        />
        <div className="flex flex-wrap items-center gap-2">
          <Button
            type="button"
            size="sm"
            disabled={!canSubmit || busy}
            onClick={() => void submit()}
          >
            {busy ? "解析打开中…" : "打开链接"}
          </Button>
          {busy ? (
            <span className="text-[11px] text-muted-foreground">
              正在解析在线视频，可能需要十几秒…
            </span>
          ) : null}
        </div>
        {openFailed ? (
          <div
            className="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-xs text-destructive"
            role="alert"
          >
            <p className="font-medium">未能打开在线视频</p>
            <p className="mt-1 leading-relaxed">{error?.message}</p>
            <p className="mt-1 text-muted-foreground">
              若提示登录或登录态失败：用浏览器扩展导出 cookies.txt → 下方导入 →
              测试可读 → 再打开链接。失败时左侧可能仍显示上一段画面。
            </p>
          </div>
        ) : null}
        {playingRemote ? (
          <div className="space-y-1 text-[11px] text-muted-foreground">
            <p>当前在线源：{currentFile}</p>
            {mediaQuery.data ? (
              <p>
                {mediaQuery.data.title ?? "在线视频"} ·{" "}
                {mediaQuery.data.chapters.length} 个章节 ·{" "}
                {mediaQuery.data.subtitles.length} 条字幕轨道
              </p>
            ) : null}
          </div>
        ) : null}
        {!openFailed && statusMessage ? (
          <p className="truncate text-[11px] text-muted-foreground">
            {statusMessage}
          </p>
        ) : null}
      </div>

      <OnlineSourceSettings />
    </div>
  );
}
