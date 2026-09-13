import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";

import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { usePlayerStore } from "@/features/player";
import { useTrackStore } from "@/features/player/trackStore";
import { loadSubtitleChoice } from "@/features/transcript/api";
import { transcriptKey } from "@/features/transcript/queries";
import { errorMessage, formatTime } from "@/lib/format";
import { cn } from "@/lib/utils";

import { createNote } from "../api";
import { useNoteComposeStore } from "../noteComposeStore";
import { notesKey } from "../queries";
import {
  activeCueListIndex,
  indicesAroundPlayback,
} from "../noteQuoteSelection";

const QUOTE_CONTEXT = 3;
const QUOTE_WINDOW = 5;

type Props = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSaved?: () => void;
  anchorRef: React.RefObject<HTMLElement | null>;
  className?: string;
};

function formatCueLabel(cue: { startMs: number; text: string }): string {
  return `${formatTime(cue.startMs)} ${cue.text.replace(/\s+/g, " ").slice(0, 36)}`;
}

/** Small popover anchored to the chat toolbar — stays inside ChatDock. */
export function QuickNoteDialog({
  open,
  onOpenChange,
  onSaved,
  anchorRef,
  className,
}: Props) {
  const queryClient = useQueryClient();
  const panelRef = useRef<HTMLDivElement>(null);
  const mediaPath = usePlayerStore((s) => s.currentFile);
  const positionMs = usePlayerStore((s) => s.currentTimeMs);
  const subtitleChoiceId = useTrackStore((s) => s.subtitleChoiceId);
  const syncMedia = useNoteComposeStore((s) => s.syncMedia);
  const selectedQuoteIndices = useNoteComposeStore((s) => s.selectedIndices);
  const anchorCueIndex = useNoteComposeStore((s) => s.anchorCueIndex);
  const setSelectedIndices = useNoteComposeStore((s) => s.setSelectedIndices);
  const setAnchorCueIndex = useNoteComposeStore((s) => s.setAnchorCueIndex);
  const toggleAtListIndex = useNoteComposeStore((s) => s.toggleAtListIndex);
  const clearQuoteSelection = useNoteComposeStore((s) => s.clearQuoteSelection);
  const [body, setBody] = useState("");
  const [includeQuotes, setIncludeQuotes] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const transcriptQuery = useQuery({
    queryKey: transcriptKey(mediaPath, subtitleChoiceId),
    queryFn: () => loadSubtitleChoice(mediaPath!, subtitleChoiceId!),
    enabled: Boolean(open && mediaPath && subtitleChoiceId),
    staleTime: Infinity,
  });

  const cues = transcriptQuery.data?.cues ?? [];
  const canAttachQuotes = Boolean(mediaPath && subtitleChoiceId && cues.length);

  const nearbyListIndices = useMemo(() => {
    if (cues.length === 0) return [];
    const center = activeCueListIndex(cues, positionMs);
    const start = Math.max(0, center - QUOTE_WINDOW);
    const end = Math.min(cues.length - 1, center + QUOTE_WINDOW);
    const indices: number[] = [];
    for (let index = start; index <= end; index += 1) {
      indices.push(index);
    }
    return indices;
  }, [cues, positionMs]);

  useEffect(() => {
    if (open && mediaPath) {
      syncMedia(mediaPath);
      setBody("");
      setIncludeQuotes(true);
      setError(null);
    }
  }, [open, mediaPath, syncMedia]);

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: MouseEvent) => {
      const target = event.target as Node;
      if (panelRef.current?.contains(target)) return;
      if (anchorRef.current?.contains(target)) return;
      onOpenChange(false);
    };
    document.addEventListener("mousedown", onPointerDown);
    return () => document.removeEventListener("mousedown", onPointerDown);
  }, [anchorRef, onOpenChange, open]);

  const fillAroundPlayback = () => {
    const indices = indicesAroundPlayback(cues, positionMs, QUOTE_CONTEXT);
    setSelectedIndices(indices);
    const center = cues[activeCueListIndex(cues, positionMs)];
    if (center) setAnchorCueIndex(center.index);
  };

  const saveMutation = useMutation({
    mutationFn: async () => {
      const trimmed = body.trim();
      if (!trimmed) {
        throw new Error("批注内容不能为空");
      }
      if (!mediaPath) {
        throw new Error("请先打开视频");
      }
      const manualQuotes =
        includeQuotes && selectedQuoteIndices.length > 0;
      await createNote({
        mediaPath,
        positionMs,
        body: trimmed,
        subtitleChoiceId: includeQuotes ? subtitleChoiceId : null,
        anchorCueIndex: manualQuotes ? anchorCueIndex : null,
        quoteCueIndices: manualQuotes ? selectedQuoteIndices : null,
        includeQuotes,
      });
    },
    onSuccess: async () => {
      if (mediaPath) {
        await queryClient.invalidateQueries({ queryKey: notesKey(mediaPath) });
      }
      onOpenChange(false);
      onSaved?.();
    },
    onError: (cause) => {
      setError(errorMessage(cause));
    },
  });

  if (!open) return null;

  const canSave = Boolean(mediaPath && body.trim()) && !saveMutation.isPending;

  return (
    <div
      ref={panelRef}
      className={cn(
        "absolute right-0 top-full z-30 mt-1.5 w-80 max-w-full rounded-lg border border-border bg-popover p-3 shadow-lg",
        className,
      )}
      role="dialog"
      aria-label="快速写批注"
    >
      <div className="mb-2 flex items-start justify-between gap-2">
        <div className="min-w-0">
          <p className="text-sm font-medium text-foreground">快速写批注</p>
          <p className="text-[11px] text-muted-foreground">
            {mediaPath
              ? `锚点 ${formatTime(positionMs)}`
              : "请先打开视频"}
          </p>
        </div>
        <Button
          type="button"
          size="icon"
          variant="ghost"
          className="size-6 shrink-0"
          aria-label="关闭"
          onClick={() => onOpenChange(false)}
        >
          <X className="size-3.5" />
        </Button>
      </div>

      <textarea
        className="mb-2 min-h-[72px] w-full resize-none rounded-md border border-input bg-background px-2 py-1.5 text-sm"
        placeholder="记录此刻的观感或要点…"
        value={body}
        disabled={!mediaPath || saveMutation.isPending}
        onChange={(event) => setBody(event.target.value)}
        onKeyDown={(event) => {
          if ((event.ctrlKey || event.metaKey) && event.key === "Enter" && canSave) {
            event.preventDefault();
            saveMutation.mutate();
          }
        }}
      />

      <label className="mb-1 flex items-center gap-2 text-[11px] text-muted-foreground">
        <input
          type="checkbox"
          checked={includeQuotes}
          disabled={!canAttachQuotes || saveMutation.isPending}
          onChange={(event) => setIncludeQuotes(event.target.checked)}
        />
        附带台词引用
        {!canAttachQuotes ? "（需先选择字幕轨）" : null}
      </label>

      {includeQuotes && canAttachQuotes ? (
        <div className="mb-2 space-y-1.5 rounded-md border border-border/60 p-2">
          <div className="flex items-center justify-between gap-2">
            <p className="text-[10px] text-muted-foreground">
              勾选台词；Shift 连选；不勾选则自动最近 3 句
            </p>
          </div>
          <div className="flex flex-wrap gap-1">
            <Button
              type="button"
              size="sm"
              variant="outline"
              className="h-6 px-2 text-[10px]"
              onClick={fillAroundPlayback}
            >
              填入±3句
            </Button>
            <Button
              type="button"
              size="sm"
              variant="ghost"
              className="h-6 px-2 text-[10px]"
              onClick={clearQuoteSelection}
            >
              清空
            </Button>
          </div>
          <ScrollArea className="h-28 rounded border border-border/40">
            <div className="space-y-0.5 p-1">
              {nearbyListIndices.map((listIndex) => {
                const cue = cues[listIndex];
                if (!cue) return null;
                const checked = selectedQuoteIndices.includes(cue.index);
                const isAnchor = anchorCueIndex === cue.index;
                return (
                  <label
                    key={cue.index}
                    className={cn(
                      "flex items-start gap-1.5 rounded px-1 py-0.5 text-[10px] hover:bg-muted/50",
                      checked && "bg-muted/30",
                    )}
                  >
                    <input
                      type="checkbox"
                      className="mt-0.5"
                      checked={checked}
                      onClick={(event) => {
                        event.preventDefault();
                        toggleAtListIndex(cues, listIndex, event.shiftKey);
                      }}
                      onChange={() => {}}
                    />
                    <span className="min-w-0 flex-1 leading-snug">
                      {formatCueLabel(cue)}
                    </span>
                    {checked ? (
                      <button
                        type="button"
                        className={cn(
                          "shrink-0 rounded px-1 text-[9px]",
                          isAnchor
                            ? "bg-primary text-primary-foreground"
                            : "text-muted-foreground hover:bg-muted",
                        )}
                        title="标为锚点句"
                        onClick={(event) => {
                          event.preventDefault();
                          setAnchorCueIndex(
                            anchorCueIndex === cue.index ? null : cue.index,
                          );
                        }}
                      >
                        锚
                      </button>
                    ) : null}
                  </label>
                );
              })}
            </div>
          </ScrollArea>
        </div>
      ) : null}

      {error ? (
        <p className="mb-2 text-[11px] text-destructive">{error}</p>
      ) : null}

      <div className="flex justify-end gap-2">
        <Button
          type="button"
          size="sm"
          variant="outline"
          disabled={saveMutation.isPending}
          onClick={() => onOpenChange(false)}
        >
          取消
        </Button>
        <Button
          type="button"
          size="sm"
          disabled={!canSave}
          onClick={() => saveMutation.mutate()}
        >
          {saveMutation.isPending ? "保存中…" : "保存"}
        </Button>
      </div>
    </div>
  );
}
