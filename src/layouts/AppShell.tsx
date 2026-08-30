import type { ReactNode } from "react";

import { usePlayerStore } from "@/features/player";

type AppShellProps = {
  children: ReactNode;
};

/** Desktop app chrome: slim title bar + main content. */
export function AppShell({ children }: AppShellProps) {
  const fileLabel = usePlayerStore((s) => {
    if (!s.currentFile) return null;
    const parts = s.currentFile.split(/[/\\]/);
    return parts[parts.length - 1] || s.currentFile;
  });
  const status = usePlayerStore((s) => s.status);

  return (
    <div className="flex h-svh flex-col overflow-hidden bg-background text-foreground">
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
        </div>
      </header>
      <main className="flex min-h-0 flex-1 flex-col">{children}</main>
    </div>
  );
}
