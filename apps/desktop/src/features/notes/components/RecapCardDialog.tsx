import { useQuery } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";

import { Button } from "@lumina/ui/button";

import { getNoteFrame } from "../api";
import { errorMessage, formatTime } from "@/lib/format";
import { cn } from "@lumina/ui/utils";

import { exportNoteRecapCard } from "../api";
import {
  decodeImage,
  RECAP_CARD_SIZE,
  renderRecapCard,
  type RecapCardData,
  type RecapCardRatio,
} from "../recapCard";
import type { Note } from "../types";

type Props = {
  note: Note;
  mediaPath: string;
  open: boolean;
  onClose: () => void;
};

/**
 * 观影打卡卡片：Note → 暗色胶片风 PNG。canvas 渲染 + 保存对话框落盘；
 * 无截帧时优雅退化为纯文字卡片。
 */
export function RecapCardDialog({ note, mediaPath, open, onClose }: Props) {
  const [ratio, setRatio] = useState<RecapCardRatio>("3:4");
  const [savedPath, setSavedPath] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const frameImageRef = useRef<HTMLImageElement | null>(null);
  const [frameReady, setFrameReady] = useState(0);

  const mediaTitle = mediaTitleOf(mediaPath);

  const frameQuery = useQuery({
    queryKey: ["recap-card-frame", note.id],
    queryFn: () => getNoteFrame(note.id),
    staleTime: Infinity,
    retry: false,
    enabled: open,
  });

  useEffect(() => {
    if (!open) return;
    const data = frameQuery.data;
    if (!data) return;
    let cancelled = false;
    void decodeImage(`data:${data.mime};base64,${data.data}`).then((image) => {
      if (cancelled) return;
      frameImageRef.current = image;
      setFrameReady((value) => value + 1);
    });
    return () => {
      cancelled = true;
    };
  }, [frameQuery.data, open]);

  useEffect(() => {
    if (!open) return;
    const canvas = canvasRef.current;
    if (!canvas) return;
    const data: RecapCardData = {
      mediaTitle,
      timeLabel: formatTime(note.positionMs),
      body: note.body,
      quotes: (note.quotes ?? []).map((quote) => quote.text),
      frameImage: frameImageRef.current,
      ratio,
    };
    try {
      renderRecapCard(canvas, data);
    } catch (renderError) {
      setError(errorMessage(renderError));
    }
  }, [open, note.body, note.positionMs, note.quotes, mediaTitle, ratio, frameReady]);

  if (!open) return null;

  const exportCard = async () => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    setBusy(true);
    setError(null);
    try {
      const dataUrl = canvas.toDataURL("image/png");
      const pngBase64 = dataUrl.replace(/^data:image\/png;base64,/, "");
      const saved = await exportNoteRecapCard(
        mediaPath,
        formatTime(note.positionMs),
        pngBase64,
      );
      if (saved) setSavedPath(saved);
    } catch (exportError) {
      setError(errorMessage(exportError));
    } finally {
      setBusy(false);
    }
  };

  const size = RECAP_CARD_SIZE[ratio];
  return (
    <div
      role="dialog"
      aria-label="打卡卡片"
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-4"
      onClick={onClose}
    >
      <div
        className="flex max-h-full flex-col items-center gap-3 overflow-auto rounded-lg border border-border bg-background p-4"
        onClick={(event) => event.stopPropagation()}
      >
        <p className="text-xs font-medium text-muted-foreground">打卡卡片预览</p>
        <canvas
          ref={canvasRef}
          className="max-h-[60vh] w-auto rounded-md border border-border"
          style={{ aspectRatio: `${size.width} / ${size.height}` }}
        />
        <div className="flex items-center gap-2">
          <select
            className={cn(
              "h-8 rounded-md border border-border bg-background px-2 text-xs",
            )}
            value={ratio}
            onChange={(event) => setRatio(event.target.value as RecapCardRatio)}
          >
            <option value="3:4">3:4</option>
            <option value="9:16">9:16</option>
          </select>
          <Button size="sm" disabled={busy} onClick={() => void exportCard()}>
            {busy ? "导出中…" : "导出 PNG"}
          </Button>
          <Button size="sm" variant="outline" onClick={onClose}>
            关闭
          </Button>
        </div>
        {savedPath ? (
          <p className="text-[11px] text-muted-foreground">已保存至 {savedPath}</p>
        ) : null}
        {error ? <p className="text-xs text-destructive">{error}</p> : null}
      </div>
    </div>
  );
}

function mediaTitleOf(mediaPath: string): string {
  const normalized = mediaPath.replace(/\\/g, "/");
  const baseName = normalized.slice(normalized.lastIndexOf("/") + 1);
  const stem = baseName.replace(/\.[a-z0-9]{2,5}$/i, "");
  return stem || "Lumina";
}
