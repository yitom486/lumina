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
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { ChatToggleButton } from "@/features/acp/components/ChatToggleButton";
import { usePlayerStore, useUiStore } from "@/features/player";
import { errorMessage } from "@/lib/format";
import { revealLogDir } from "@/lib/system";

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
          size="icon-sm"
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
          size="icon-sm"
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
              size="icon-sm"
              aria-label="关于 Lumina"
            >
              <Info className="size-4" />
            </Button>
          </AlertDialogTrigger>
        </TooltipTrigger>
        <TooltipContent>关于 Lumina</TooltipContent>
      </Tooltip>
      <AlertDialogContent>
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
  const fullscreen = useUiStore((s) => s.fullscreen);
  const fileLabel = usePlayerStore((s) => {
    if (!s.currentFile) return null;
    const parts = s.currentFile.split(/[/\\]/);
    return parts[parts.length - 1] || s.currentFile;
  });
  const status = usePlayerStore((s) => s.status);
  const playlist = usePlayerStore((s) => s.playlist);
  const playlistIndex = usePlayerStore((s) => s.playlistIndex);

  return (
    <div className="relative flex h-svh flex-col overflow-hidden bg-background text-foreground">
      {!fullscreen ? (
        <header className="flex h-11 shrink-0 items-center gap-3 border-b border-border bg-card px-4">
          <div className="flex min-w-0 items-baseline gap-2">
            <p className="text-sm font-semibold tracking-tight">Lumina</p>
            <p className="hidden text-xs text-muted-foreground sm:inline">
              AI Video Reader
            </p>
          </div>
          <div className="min-w-0 flex-1 truncate text-center text-xs text-muted-foreground">
            {fileLabel ?? "未打开文件"}
            {status && fileLabel ? (
              <span className="ml-2 opacity-70">· {status}</span>
            ) : null}
            {playlist.length > 1 && playlistIndex >= 0 ? (
              <span className="ml-2 opacity-70">
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
