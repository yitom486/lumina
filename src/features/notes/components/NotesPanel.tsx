import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";

import { Button } from "@/components/ui/button";
import { usePlayerStore } from "@/features/player";
import { errorMessage, formatTime } from "@/lib/format";
import { cn } from "@/lib/utils";

import {
  createNote,
  deleteNote,
  exportNotesMarkdown,
  listNotes,
} from "../api";

export function NotesPanel() {
  const mediaPath = usePlayerStore((s) => s.currentFile);
  const positionMs = usePlayerStore((s) => s.currentTimeMs);
  const seek = usePlayerStore((s) => s.seek);
  const queryClient = useQueryClient();
  const [body, setBody] = useState("");
  const [exportText, setExportText] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const notesQuery = useQuery({
    queryKey: ["notes", mediaPath],
    queryFn: () => listNotes(mediaPath!),
    enabled: Boolean(mediaPath),
  });

  const createMutation = useMutation({
    mutationFn: createNote,
    onSuccess: async () => {
      setBody("");
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

  if (!mediaPath) {
    return (
      <div className="p-3 text-xs text-muted-foreground">打开视频后可记录带时间戳的笔记。</div>
    );
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-2 p-3">
      <div className="flex items-center justify-between gap-2">
        <span className="text-xs text-muted-foreground">
          当前 {formatTime(positionMs)}
        </span>
        <Button
          size="sm"
          variant="outline"
          onClick={async () => {
            try {
              const md = await exportNotesMarkdown(mediaPath);
              setExportText(md);
              await navigator.clipboard.writeText(md);
            } catch (err) {
              setError(errorMessage(err));
            }
          }}
        >
          导出 MD
        </Button>
      </div>

      <textarea
        className={cn(
          "min-h-[64px] w-full resize-none rounded-md border border-border bg-background px-2 py-1.5 text-sm",
          "outline-none focus-visible:ring-1 focus-visible:ring-ring",
        )}
        placeholder="记一笔…"
        value={body}
        onChange={(e) => setBody(e.target.value)}
      />
      <Button
        size="sm"
        disabled={!body.trim() || createMutation.isPending}
        onClick={() =>
          createMutation.mutate({
            mediaPath,
            positionMs,
            body: body.trim(),
          })
        }
      >
        保存笔记
      </Button>

      {error ? <p className="text-xs text-destructive">{error}</p> : null}

      <div className="min-h-0 flex-1 space-y-1 overflow-auto">
        {(notesQuery.data ?? []).map((note) => (
          <div
            key={note.id}
            className="flex items-start gap-2 rounded-md border border-border px-2 py-1.5 text-xs"
          >
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
        ))}
        {notesQuery.data?.length === 0 ? (
          <p className="text-muted-foreground">暂无笔记</p>
        ) : null}
      </div>

      {exportText ? (
        <pre className="max-h-28 overflow-auto rounded-md bg-muted/40 p-2 text-[10px] whitespace-pre-wrap">
          {exportText}
        </pre>
      ) : null}
    </div>
  );
}
