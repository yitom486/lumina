import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { Button } from "@/components/ui/button";

type PlayerSnapshot = {
  status: string;
  currentTimeMs: number;
  durationMs: number;
  volume: number;
  rate: number;
  currentFile: string | null;
  error: { code: string; message: string; details?: string } | null;
};

export default function App() {
  const surfaceRef = useRef<HTMLDivElement>(null);
  const [snapshot, setSnapshot] = useState<PlayerSnapshot | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("选择本地视频以验证 native libmpv 画面");

  const reportBounds = useCallback(async () => {
    const el = surfaceRef.current;
    if (!el) return;
    const rect = el.getBoundingClientRect();
    try {
      await invoke("player_set_surface_bounds", {
        x: rect.left,
        y: rect.top,
        width: rect.width,
        height: rect.height,
      });
    } catch (error) {
      console.error("set surface bounds failed", error);
    }
  }, []);

  useEffect(() => {
    void reportBounds();
    const el = surfaceRef.current;
    if (!el) return;

    const observer = new ResizeObserver(() => {
      void reportBounds();
    });
    observer.observe(el);
    window.addEventListener("resize", reportBounds);
    return () => {
      observer.disconnect();
      window.removeEventListener("resize", reportBounds);
    };
  }, [reportBounds]);

  async function handleOpen() {
    setBusy(true);
    try {
      const selected = await open({
        multiple: false,
        filters: [
          {
            name: "Video",
            extensions: ["mp4", "mkv", "webm", "avi", "mov", "m4v"],
          },
        ],
      });
      if (!selected || Array.isArray(selected)) {
        setBusy(false);
        return;
      }

      await reportBounds();
      const next = await invoke<PlayerSnapshot>("player_open", { path: selected });
      setSnapshot(next);
      setMessage(next.currentFile ? `Playing: ${next.currentFile}` : "Opened");
    } catch (error) {
      const text =
        typeof error === "object" && error && "message" in error
          ? String((error as { message: string }).message)
          : String(error);
      setMessage(text);
    } finally {
      setBusy(false);
    }
  }

  async function handlePlay() {
    try {
      setSnapshot(await invoke<PlayerSnapshot>("player_play"));
    } catch (error) {
      setMessage(String(error));
    }
  }

  async function handlePause() {
    try {
      setSnapshot(await invoke<PlayerSnapshot>("player_pause"));
    } catch (error) {
      setMessage(String(error));
    }
  }

  return (
    <div className="flex min-h-svh flex-col bg-background text-foreground">
      <header className="border-b border-border px-6 py-3">
        <p className="text-lg font-semibold tracking-tight">Lumina</p>
        <p className="mt-1 text-sm text-muted-foreground">
          Phase 1 — Native Playback Spike
        </p>
      </header>
      <main className="flex flex-1 flex-col">
        <div
          ref={surfaceRef}
          className="relative min-h-0 flex-1 bg-transparent"
          aria-label="Native video surface"
        />
        <div className="flex flex-wrap items-center gap-3 border-t border-border px-6 py-4">
          <Button type="button" onClick={() => void handleOpen()} disabled={busy}>
            Open
          </Button>
          <Button
            type="button"
            variant="outline"
            onClick={() => void handlePlay()}
            disabled={busy}
          >
            Play
          </Button>
          <Button
            type="button"
            variant="outline"
            onClick={() => void handlePause()}
            disabled={busy}
          >
            Pause
          </Button>
          <p className="text-sm text-muted-foreground">
            {snapshot ? `${snapshot.status}` : "Idle"} · {message}
          </p>
        </div>
      </main>
    </div>
  );
}
