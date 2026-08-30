/** MX Player-style resume chip: non-blocking, auto-hides after 4s. */

import { useEffect } from "react";
import { X } from "lucide-react";

import { formatTime } from "@/lib/format";
import { Button } from "@/components/ui/button";

import { usePlayerStore } from "../store";

const AUTO_DISMISS_MS = 4_000;

export function ResumeChip() {
  const toast = usePlayerStore((s) => s.resumeToast);
  const dismissResumeToast = usePlayerStore((s) => s.dismissResumeToast);
  const restartFromBeginning = usePlayerStore((s) => s.restartFromBeginning);

  useEffect(() => {
    if (!toast) return;
    const id = window.setTimeout(() => {
      dismissResumeToast();
    }, AUTO_DISMISS_MS);
    return () => window.clearTimeout(id);
  }, [toast, dismissResumeToast]);

  if (!toast) return null;

  return (
    <div
      className="mb-1.5 flex max-w-full items-center gap-2 self-start rounded-md border border-border bg-secondary/95 px-2.5 py-1 text-xs text-secondary-foreground shadow-sm"
      role="status"
    >
      <span className="truncate text-muted-foreground">
        已从 {formatTime(toast.positionMs)} 继续
      </span>
      <Button
        type="button"
        variant="outline"
        size="sm"
        className="h-6 shrink-0 px-2 text-xs"
        onClick={() => void restartFromBeginning()}
      >
        从头播放
      </Button>
      <button
        type="button"
        className="rounded p-0.5 text-muted-foreground hover:bg-accent hover:text-foreground"
        aria-label="关闭"
        onClick={() => dismissResumeToast()}
      >
        <X className="size-3.5" />
      </button>
    </div>
  );
}
