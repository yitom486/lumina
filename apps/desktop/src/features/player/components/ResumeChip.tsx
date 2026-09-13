/** MX Player-style resume chip: overlay only — must not reflow PlayerBar. */

import { useEffect } from "react";
import { X } from "lucide-react";

import { formatTime } from "@/lib/format";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

import { usePlayerStore } from "../store";
import { useUiStore } from "../uiStore";

const AUTO_DISMISS_MS = 4_000;

export function ResumeChip() {
  const toast = usePlayerStore((s) => s.resumeToast);
  const cinema = useUiStore((s) => s.fullscreen);
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
      className="pointer-events-none absolute inset-0 z-40 flex items-center justify-center"
      role="status"
    >
      <div
        className={cn(
          "pointer-events-auto flex max-w-full items-center gap-2 rounded-md border px-2.5 py-1 text-xs shadow-md",
          cinema
            ? "border-white/15 bg-black/85 text-foreground backdrop-blur-sm"
            : "border-border/80 bg-muted/95 text-secondary-foreground",
        )}
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
    </div>
  );
}