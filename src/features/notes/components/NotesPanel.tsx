import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useMemo, useRef, useState } from "react";

import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { usePlayerStore } from "@/features/player";
import { useTrackStore } from "@/features/player/trackStore";
import { loadSubtitleChoice } from "@/features/transcript/api";
import type { Cue } from "@/features/transcript/types";
import { errorMessage, formatTime } from "@/lib/format";
import { cn } from "@/lib/utils";

import {
  createNote,
  deleteNote,
  exportNotesMarkdownToFile,
  listNotes,
  previewNoteQuotes,
} from "../api";
import { useNoteComposeStore } from "../noteComposeStore";
import {
  activeCueListIndex,
  indicesAroundPlayback,
} from "../noteQuoteSelection";
import type { NoteQuote } from "../types";

const QUOTE_CONTEXT = 3;

function formatCueLabel(cue: Cue): string {
  return `${formatTime(cue.startMs)} ${cue.text.replace(/\s+/g, " ").slice(0, 48)}`;
}

export function NotesPanel() {
  const mediaPath = usePlayerStore((s) => s.currentFile);
  const positionMs = usePlayerStore((s) => s.currentTimeMs);
  const seek = usePlayerStore((s) => s.seek);
  const subtitleChoiceId = useTrackStore((s) => s.subtitleChoiceId);
  const queryClient = useQueryClient();
  const [quotePreview, setQuotePreview] = useState<NoteQuote[]>([]);
  const [exportSavedPath, setExportSavedPath] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const activeCueRef = useRef<HTMLLabelElement | null>(null);

  const body = useNoteComposeStore((s) => s.body);
  const includeQuotes = useNoteComposeStore((s) => s.includeQuotes);
  const quoteMode = useNoteComposeStore((s) => s.quoteMode);
  const selectedQuoteIndices = useNoteComposeStore((s) => s.selectedIndices);
  const anchorCueIndex = useNoteComposeStore((s) => s.anchorCueIndex);
  const setBody = useNoteComposeStore((s) => s.setBody);
  const setIncludeQuotes = useNoteComposeStore((s) => s.setIncludeQuotes);
  const syncMedia = useNoteComposeStore((s) => s.syncMedia);
  const setQuoteMode = useNoteComposeStore((s) => s.setQuoteMode);
  const setSelectedIndices = useNoteComposeStore((s) => s.setSelectedIndices);
  const toggleAtListIndex = useNoteComposeStore((s) => s.toggleAtListIndex);
  const setAnchorCueIndex = useNoteComposeStore((s) => s.setAnchorCueIndex);
  const clearQuoteSelection = useNoteComposeStore((s) => s.clearQuoteSelection);
  const resetCompose = useNoteComposeStore((s) => s.resetCompose);
  const resetComposeKeepQuotes = useNoteComposeStore(
    (s) => s.resetComposeKeepQuotes,
  );

  const transcriptQuery = useQuery({
    queryKey: ["transcript", mediaPath, subtitleChoiceId],
    queryFn: () => loadSubtitleChoice(mediaPath!, subtitleChoiceId!),
    enabled: Boolean(mediaPath && subtitleChoiceId),
    staleTime: Infinity,
  });

  const cues = transcriptQuery.data?.cues ?? [];

  const notesQuery = useQuery({
    queryKey: ["notes", mediaPath],
    queryFn: () => listNotes(mediaPath!),
    enabled: Boolean(mediaPath),
  });

  useEffect(() => {
    if (mediaPath) {
      syncMedia(mediaPath);
    }
  }, [mediaPath, syncMedia]);

  useEffect(() => {
    activeCueRef.current?.scrollIntoView({ block: "nearest" });
  }, [quoteMode, positionMs, cues.length]);

  const playbackCueIndex = cues.length
    ? activeCueListIndex(cues, positionMs)
    : -1;

  const previewInput = useMemo(() => {
    if (!mediaPath || !includeQuotes || !subtitleChoiceId) return null;
    if (quoteMode === "manual") {
      return {
        mediaPath,
        positionMs,
        subtitleChoiceId,
        anchorCueIndex,
        quoteCueIndices:
          selectedQuoteIndices.length > 0 ? selectedQuoteIndices : null,
      };
    }
    return {
      mediaPath,
      positionMs,
      subtitleChoiceId,
      anchorCueIndex: null,
      quoteCueIndices: null,
    };
  }, [
    mediaPath,
    positionMs,
    subtitleChoiceId,
    includeQuotes,
    quoteMode,
    anchorCueIndex,
    selectedQuoteIndices,
  ]);

  useEffect(() => {
    if (!previewInput) {
      setQuotePreview([]);
      return;
    }
    let cancelled = false;
    void previewNoteQuotes(previewInput)
      .then((quotes) => {
        if (!cancelled) setQuotePreview(quotes);
      })
      .catch(() => {
        if (!cancelled) setQuotePreview([]);
      });
    return () => {
      cancelled = true;
    };
  }, [previewInput]);

  const createMutation = useMutation({
    mutationFn: createNote,
    onSuccess: async () => {
      resetComposeKeepQuotes();
      setError(null);
      await queryClient.invalidateQueries({ queryKey: ["notes", mediaPath] });
    },
    onError: (err) => setError(errorMessage(err)),
  });

  const deleteMutation = useMutation({
    mutationFn: deleteNote,
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["notes", mediaPath] });
    },
    onError: (err) => setError(errorMessage(err)),
  });

  const fillAroundPlayback = () => {
    const indices = indicesAroundPlayback(cues, positionMs, QUOTE_CONTEXT);
    setSelectedIndices(indices);
    const center = cues[playbackCueIndex];
    if (center) setAnchorCueIndex(center.index);
    setQuoteMode("manual");
  };

  if (!mediaPath) {
    return (
      <div className="p-3 text-xs text-muted-foreground">
        打开视频后可记录带时间戳的笔记。
      </div>
    );
  }

  const canAttachQuotes = Boolean(subtitleChoiceId && cues.length);
  const composing = body.trim().length > 0 || selectedQuoteIndices.length > 0;

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-2 p-3">
      <div className="flex items-center justify-between gap-2">
        <span className="text-xs text-muted-foreground">
          当前 {formatTime(positionMs)}
        </span>
        <div className="flex gap-1">
          <Button
            size="sm"
            variant="ghost"
            className="h-7 text-[11px]"
            onClick={() => resetCompose()}
          >
            新建批注
          </Button>
          <Button
            size="sm"
            variant="outline"
            onClick={async () => {
              try {
                const saved = await exportNotesMarkdownToFile(mediaPath);
                if (saved) {
                  setExportSavedPath(saved);
                  setError(null);
                }
              } catch (err) {
                setError(errorMessage(err));
              }
            }}
          >
            导出 MD
          </Button>
        </div>
      </div>

      <div className="rounded-md border border-dashed border-border/80 p-2">
        <p className="mb-2 text-[11px] text-muted-foreground">
          {composing ? "正在编写（未保存）" : "新建批注"}
          {" · "}
          保存后写入下方列表；同一段台词可写多条批注
        </p>

        <textarea
          className={cn(
            "min-h-[64px] w-full resize-none rounded-md border border-border bg-background px-2 py-1.5 text-sm",
            "outline-none focus-visible:ring-1 focus-visible:ring-ring",
          )}
          placeholder="写下这一刻的感想…"
          value={body}
          onChange={(e) => setBody(e.target.value)}
        />

        <label className="mt-2 flex items-center gap-2 text-xs text-muted-foreground">
          <input
            type="checkbox"
            checked={includeQuotes}
            disabled={!canAttachQuotes}
            onChange={(e) => setIncludeQuotes(e.target.checked)}
          />
          附带台词引用
          {!canAttachQuotes ? "（需先选择字幕轨）" : null}
        </label>

        {includeQuotes && canAttachQuotes ? (
          <div className="mt-2 space-y-2 rounded-md border border-border/60 p-2">
            <div className="flex flex-wrap items-center gap-2">
              <span className="text-[11px] text-muted-foreground">引用方式</span>
              <div className="flex rounded-md border border-border p-0.5 text-[11px]">
                <button
                  type="button"
                  className={cn(
                    "rounded px-2 py-0.5",
                    quoteMode === "auto" && "bg-muted font-medium",
                  )}
                  onClick={() => setQuoteMode("auto")}
                >
                  自动（最近三句）
                </button>
                <button
                  type="button"
                  className={cn(
                    "rounded px-2 py-0.5",
                    quoteMode === "manual" && "bg-muted font-medium",
                  )}
                  onClick={() => {
                    setQuoteMode("manual");
                    if (selectedQuoteIndices.length === 0) {
                      fillAroundPlayback();
                    }
                  }}
                >
                  手动勾选
                </button>
              </div>
            </div>

            {quoteMode === "manual" ? (
              <>
                <p className="text-[11px] text-muted-foreground">
                  可勾选任意多句；Shift 连选；文稿「引用」可带入
                </p>
                <div className="flex flex-wrap gap-2">
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    className="h-7 text-[11px]"
                    onClick={fillAroundPlayback}
                  >
                    填入当前±3句
                  </Button>
                  <Button
                    type="button"
                    size="sm"
                    variant="ghost"
                    className="h-7 text-[11px]"
                    onClick={clearQuoteSelection}
                  >
                    清空引用
                  </Button>
                </div>
                <ScrollArea className="h-40 rounded border border-border/40">
                  <div className="space-y-0.5 p-1">
                    {cues.map((cue, index) => {
                      const checked = selectedQuoteIndices.includes(cue.index);
                      const isAnchor = anchorCueIndex === cue.index;
                      return (
                        <label
                          key={cue.index}
                          ref={
                            index === playbackCueIndex ? activeCueRef : undefined
                          }
                          className={cn(
                            "flex items-start gap-2 rounded px-1.5 py-1 text-[11px] hover:bg-muted/50",
                            index === playbackCueIndex && "bg-primary/5",
                            checked && "bg-muted/30",
                          )}
                        >
                          <input
                            type="checkbox"
                            className="mt-0.5"
                            checked={checked}
                            onClick={(event) => {
                              event.preventDefault();
                              toggleAtListIndex(cues, index, event.shiftKey);
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
                                "shrink-0 rounded px-1 text-[10px]",
                                isAnchor
                                  ? "bg-primary text-primary-foreground"
                                  : "text-muted-foreground hover:bg-muted",
                              )}
                              title="标为锚点句"
                              onClick={(event) => {
                                event.preventDefault();
                                setAnchorCueIndex(
                                  anchorCueIndex === cue.index
                                    ? null
                                    : cue.index,
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
              </>
            ) : (
              <p className="text-[11px] text-muted-foreground">
                将引用当前播放位置之前的最近 3 句字幕
              </p>
            )}

            {quotePreview.length > 0 ? (
              <ScrollArea className="h-24 rounded border border-border/40">
                <div className="space-y-0.5 p-2 text-[11px] text-muted-foreground">
                  {quotePreview.map((quote) => (
                    <p
                      key={`${quote.index}-${quote.startMs}`}
                      className={cn(
                        quote.anchor && "font-medium text-foreground",
                      )}
                    >
                      {quote.anchor ? "▸ " : "· "}
                      {quote.text}
                    </p>
                  ))}
                </div>
              </ScrollArea>
            ) : quoteMode === "manual" ? (
              <p className="text-[11px] text-muted-foreground">
                请勾选要引用的台词
              </p>
            ) : (
              <p className="text-[11px] text-muted-foreground">
                此位置附近暂无可引用台词
              </p>
            )}
          </div>
        ) : null}

        <Button
          size="sm"
          className="mt-2 w-full"
          disabled={!body.trim() || createMutation.isPending}
          onClick={() =>
            createMutation.mutate({
              mediaPath,
              positionMs,
              body: body.trim(),
              subtitleChoiceId: includeQuotes ? subtitleChoiceId : null,
              anchorCueIndex:
                includeQuotes && quoteMode === "manual" ? anchorCueIndex : null,
              quoteCueIndices:
                includeQuotes && quoteMode === "manual"
                  ? selectedQuoteIndices
                  : null,
              includeQuotes,
            })
          }
        >
          保存批注
        </Button>
      </div>

      {error ? <p className="text-xs text-destructive">{error}</p> : null}

      <p className="text-[11px] font-medium text-muted-foreground">已保存</p>
      <div className="min-h-0 flex-1 space-y-1 overflow-auto">
        {(notesQuery.data ?? []).map((note) => (
          <div
            key={note.id}
            className="flex flex-col gap-1 rounded-md border border-border px-2 py-1.5 text-xs"
          >
            <div className="flex items-start gap-2">
              <button
                type="button"
                className="shrink-0 font-mono text-primary hover:underline"
                onClick={() => void seek(note.positionMs)}
              >
                {formatTime(note.positionMs)}
              </button>
              <p className="min-w-0 flex-1 whitespace-pre-wrap">{note.body}</p>
              <button
                type="button"
                className="shrink-0 text-muted-foreground hover:text-destructive"
                onClick={() => deleteMutation.mutate(note.id)}
              >
                删
              </button>
            </div>
            {note.quotes?.length ? (
              <div className="border-l-2 border-muted pl-2 text-[11px] text-muted-foreground">
                {note.quotes.map((quote) => (
                  <p
                    key={`${note.id}-${quote.index}`}
                    className={cn(quote.anchor && "font-medium text-foreground")}
                  >
                    {quote.text}
                  </p>
                ))}
              </div>
            ) : null}
          </div>
        ))}
        {notesQuery.data?.length === 0 ? (
          <p className="text-muted-foreground">暂无批注</p>
        ) : null}
      </div>

      {exportSavedPath ? (
        <p className="text-[11px] text-muted-foreground">
          已保存至 {exportSavedPath}
        </p>
      ) : null}
    </div>
  );
}
