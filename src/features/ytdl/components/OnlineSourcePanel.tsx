/** Online URL open + cookie settings — lives in sidebar (never over HWND). */

import { useState } from "react";

import { Button } from "@/components/ui/button";
import { OnlineSourceSettings } from "@/features/ytdl/components/OnlineSourceSettings";
import { errorMessage } from "@/lib/format";
import { usePlayerStore } from "@/features/player";

export function OnlineSourcePanel() {
  const busy = usePlayerStore((s) => s.busy);
  const openUrl = usePlayerStore((s) => s.openUrl);
  const statusMessage = usePlayerStore((s) => s.statusMessage);
  const [url, setUrl] = useState("");
  const [localError, setLocalError] = useState<string | null>(null);

  const canSubmit =
    /^https?:\/\//i.test(url.trim()) && url.trim().length > 10;

  const submit = async () => {
    if (!canSubmit || busy) return;
    setLocalError(null);
    try {
      await openUrl(url);
      setUrl("");
    } catch (error) {
      setLocalError(errorMessage(error));
    }
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-y-auto p-3">
      <div className="space-y-2">
        <h2 className="text-sm font-medium">打开在线视频</h2>
        <p className="text-xs text-muted-foreground">
          粘贴 YouTube / Bilibili 等页面链接。此面板在侧栏内，不会被视频画面挡住。
        </p>
        <input
          type="url"
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          placeholder="https://"
          className="w-full rounded-md border border-border bg-background px-3 py-2 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
          disabled={busy}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              void submit();
            }
          }}
        />
        <div className="flex items-center gap-2">
          <Button
            type="button"
            size="sm"
            disabled={!canSubmit || busy}
            onClick={() => void submit()}
          >
            {busy ? "打开中…" : "打开链接"}
          </Button>
          {statusMessage ? (
            <span className="truncate text-[11px] text-muted-foreground">
              {statusMessage}
            </span>
          ) : null}
        </div>
        {localError ? (
          <p className="text-xs text-destructive">{localError}</p>
        ) : null}
      </div>

      <OnlineSourceSettings />
    </div>
  );
}
