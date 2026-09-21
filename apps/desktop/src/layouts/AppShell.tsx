import { useEffect, useState, type ReactNode } from "react";
import { FileText, Info, Maximize2, Minimize2 } from "lucide-react";
import { getVersion } from "@tauri-apps/api/app";
import { openUrl } from "@tauri-apps/plugin-opener";

import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "@lumina/ui/alert-dialog";
import { Button } from "@lumina/ui/button";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@lumina/ui/tooltip";
import { ChatToggleButton } from "@lumina/chat-ui/components/ChatToggleButton";
import { usePlayerStore, useUiStore } from "@/features/player";
import { errorMessage } from "@/lib/format";
import { getStartupNotice, revealLogDir, type SystemStartupNotice } from "@/lib/system";
import luminaLogo from "../../src-tauri/icons/lumina-cat-source.png";

const RELEASES_URL = "https://github.com/yitom486/lumina-app/releases";

type AppShellProps = {
  children: ReactNode;
};

export function FullscreenToggleButton() {
  const fullscreen = useUiStore((s) => s.fullscreen);
  const toggleFullscreen = useUiStore((s) => s.toggleFullscreen);

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size="icon" className="size-8"
          onClick={() => void toggleFullscreen()}
          aria-label={fullscreen ? "退出全屏" : "全屏"}
        >
          {fullscreen ? (
            <Minimize2 className="size-4" />
          ) : (
            <Maximize2 className="size-4" />
          )}
        </Button>
      </TooltipTrigger>
      <TooltipContent>
        {fullscreen ? "退出全屏 (Esc / F)" : "全屏 (F)"}
      </TooltipContent>
    </Tooltip>
  );
}

/** Crash/log export: reveal the file-log directory in the OS file manager. */
function LogDirButton() {
  const onClick = () => {
    void revealLogDir().catch((error: unknown) => {
      usePlayerStore.getState().setStatusMessage(errorMessage(error));
    });
  };

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size="icon" className="size-8"
          onClick={onClick}
          aria-label="打开日志目录"
        >
          <FileText className="size-4" />
        </Button>
      </TooltipTrigger>
      <TooltipContent>打开日志目录（报障时打包发开发者）</TooltipContent>
    </Tooltip>
  );
}

/** About dialog: product intro + runtime version + update entry. */
export function AboutButton() {
  const [version, setVersion] = useState("…");
  useEffect(() => {
    let cancelled = false;
    void getVersion()
      .then((value) => {
        if (!cancelled) setVersion(value);
      })
      .catch(() => {
        if (!cancelled) setVersion("未知");
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const openReleases = () => {
    void openUrl(RELEASES_URL).catch((error: unknown) => {
      usePlayerStore.getState().setStatusMessage(errorMessage(error));
    });
  };

  return (
    <AlertDialog>
      <Tooltip>
        <TooltipTrigger asChild>
          <AlertDialogTrigger asChild>
            <Button
              type="button"
              variant="ghost"
              size="icon" className="size-8"
              aria-label="关于 Lumina"
            >
              <Info className="size-4" />
            </Button>
          </AlertDialogTrigger>
        </TooltipTrigger>
        <TooltipContent>关于 Lumina</TooltipContent>
      </Tooltip>
      {/* vendor 还原后默认 bg-background/max-w-lg；转出层保留原深色窄版。 */}
      <AlertDialogContent className="border-border bg-card max-w-md">
        <AlertDialogHeader>
          <AlertDialogTitle>Lumina · AI Video Reader</AlertDialogTitle>
          <AlertDialogDescription>版本 {version}</AlertDialogDescription>
        </AlertDialogHeader>
        <p className="text-sm text-muted-foreground">
          桌面端 AI
          观影阅读器：用原生播放器播放本地视频，把字幕文稿、章节、笔记与可选的
          AI 对话放在同一个阅读工作流里。
        </p>
        <AlertDialogFooter>
          <Button type="button" variant="outline" onClick={openReleases}>
            下载更新
          </Button>
          <AlertDialogAction>知道了</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

/** Desktop app chrome: slim title bar + main content. */
export function AppShell({ children }: AppShellProps) {
  const [startupNotice, setStartupNotice] = useState<SystemStartupNotice | null>(null);
  const fullscreen = useUiStore((s) => s.fullscreen);
  const fileLabel = usePlayerStore((s) => {
    if (!s.currentFile) return null;
    const parts = s.currentFile.split(/[/\\]/);
    return parts[parts.length - 1] || s.currentFile;
  });
  const fileFormat = fileLabel?.match(/\.([^.]+)$/)?.[1]?.toUpperCase() ?? null;
  const status = usePlayerStore((s) => s.status);
  const playlist = usePlayerStore((s) => s.playlist);
  const playlistIndex = usePlayerStore((s) => s.playlistIndex);

  useEffect(() => {
    let cancelled = false;
    void getStartupNotice()
      .then((notice) => {
        if (!cancelled) setStartupNotice(notice);
      })
      .catch(() => {
        if (!cancelled) setStartupNotice(null);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <div className="relative flex h-svh flex-col overflow-hidden bg-background text-foreground">
      {startupNotice ? (
        <div
          role="status"
          className="absolute inset-x-3 top-3 z-50 flex items-center justify-between gap-3 rounded-lg border border-border bg-card px-3 py-2 text-xs shadow-lg"
        >
          <span className="text-muted-foreground">{startupNotice.message}</span>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={() => setStartupNotice(null)}
          >
            知道了
          </Button>
        </div>
      ) : null}
      {!fullscreen ? (
        <header
          aria-label="应用标题栏"
          className="flex h-14 shrink-0 items-center gap-3 border-b border-border bg-surface-elevated px-3 shadow-sm"
        >
          <div className="flex min-w-0 shrink-0 items-center gap-2.5">
            <img
              src={luminaLogo}
              alt="Lumina"
              className="size-9 rounded-xl border border-ai/40 object-cover shadow-sm"
            />
            <div className="flex min-w-0 items-baseline gap-2">
              <p className="text-base font-semibold tracking-tight text-surface-foreground">
                Lumina
              </p>
              <p className="hidden text-xs text-muted-foreground sm:inline">
                AI Video Reader
              </p>
            </div>
          </div>
          <div aria-hidden="true" className="h-6 w-px shrink-0 bg-border" />
          <div className="flex min-w-0 flex-1 items-center justify-center gap-2 text-xs text-muted-foreground">
            <span className="truncate" title={fileLabel ?? undefined}>
              {fileLabel ?? "未打开文件"}
            </span>
            {fileFormat ? (
              <span className="shrink-0 rounded border border-border/80 bg-muted px-1.5 py-0.5 text-[10px] font-medium tracking-wide text-muted-foreground">
                {fileFormat}
              </span>
            ) : null}
            {status && fileLabel ? <span className="shrink-0 opacity-70">· {status}</span> : null}
            {playlist.length > 1 && playlistIndex >= 0 ? (
              <span className="shrink-0 opacity-70">
                · {playlistIndex + 1}/{playlist.length}
              </span>
            ) : null}
          </div>
          <div className="flex shrink-0 items-center gap-0.5">
            <ChatToggleButton />
            <LogDirButton />
            <AboutButton />
            <FullscreenToggleButton />
          </div>
        </header>
      ) : null}
      <main className="flex min-h-0 flex-1 flex-col">{children}</main>
    </div>
  );
}
