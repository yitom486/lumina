/** Online URL open + cookie settings — lives in sidebar (never over HWND). */

import { useState } from "react";

import { Button } from "@/components/ui/button";
import { OnlineSourceSettings } from "@/features/ytdl/components/OnlineSourceSettings";
import { usePlayerStore } from "@/features/player";

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

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-y-auto p-3">
      <div className="space-y-2">
        <h2 className="text-sm font-medium">打开在线视频</h2>
        <p className="text-xs text-muted-foreground">
          粘贴完整链接后点「打开链接」。首次需先安装下方的在线解析组件；失败时左侧可能仍显示上一段画面。
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
              请先确认下方已安装解析组件；若需更高清晰度，再授权浏览器登录态后重试。
            </p>
          </div>
        ) : null}
        {playingRemote ? (
          <p className="text-[11px] text-muted-foreground">
            当前在线源：{currentFile}
          </p>
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
