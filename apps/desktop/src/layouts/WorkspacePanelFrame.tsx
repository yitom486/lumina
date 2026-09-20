import type { ReactNode } from "react";

import { cn } from "@lumina/ui/utils";

type WorkspacePanelFrameProps = {
  title: string;
  subtitle?: ReactNode;
  icon?: ReactNode;
  actions?: ReactNode;
  children: ReactNode;
  className?: string;
  contentClassName?: string;
};

/** Shared right-side workspace shell for ordinary panels and the AI dock. */
export function WorkspacePanelFrame({
  title,
  subtitle,
  icon,
  actions,
  children,
  className,
  contentClassName,
}: WorkspacePanelFrameProps) {
  return (
    <section
      aria-label={title}
      className={cn(
        "relative z-10 flex min-h-0 min-w-0 flex-col overflow-hidden border-l border-border bg-card",
        className,
      )}
    >
      <header className="flex min-h-10 shrink-0 items-center justify-between gap-2 border-b border-border px-3 py-2">
        <div className="flex min-w-0 items-center gap-1.5">
          {icon ? <span className="shrink-0 text-ai">{icon}</span> : null}
          <div className="min-w-0">
            <p className="truncate text-xs font-medium">{title}</p>
            {subtitle ? (
              <p className="truncate text-[10px] text-muted-foreground">
                {subtitle}
              </p>
            ) : null}
          </div>
        </div>
        {actions ? <div className="flex shrink-0 items-center gap-1">{actions}</div> : null}
      </header>
      <div className={cn("flex min-h-0 flex-1 flex-col overflow-hidden", contentClassName)}>
        {children}
      </div>
    </section>
  );
}
