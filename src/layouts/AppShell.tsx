import type { ReactNode } from "react";

type AppShellProps = {
  children: ReactNode;
};

/** Desktop app chrome: title bar region + main content column. */
export function AppShell({ children }: AppShellProps) {
  return (
    <div className="flex min-h-svh flex-col bg-background text-foreground">
      <header className="border-b border-border px-6 py-3">
        <p className="text-lg font-semibold tracking-tight">Lumina</p>
        <p className="mt-1 text-sm text-muted-foreground">
          AI Video Reader — Subtitle / Transcript
        </p>
      </header>
      <main className="flex flex-1 flex-col">{children}</main>
    </div>
  );
}
