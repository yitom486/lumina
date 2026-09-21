import { Eye, EyeOff } from "lucide-react";
import { useState, type ReactNode } from "react";

import { Button, cn } from "@lumina/ui";

import type { SpoilerLevel } from "../assistantBlocks";

type Props = {
  level: SpoilerLevel;
  children: ReactNode;
  className?: string;
};

/** Keeps future-spoiler content out of the DOM until the viewer opts in. */
export function SpoilerGuard({ level, children, className }: Props) {
  const [revealed, setRevealed] = useState(false);

  if (level !== "future") return <>{children}</>;

  return (
    <div
      className={cn("rounded-md border border-border bg-muted/30 p-2", className)}
      data-spoiler-state={revealed ? "revealed" : "hidden"}
    >
      {revealed ? (
        <div className="space-y-2">
          {children}
          <Button
            type="button"
            size="sm"
            variant="ghost"
            className="h-7 px-2 text-xs"
            onClick={() => setRevealed(false)}
          >
            <EyeOff aria-hidden />
            隐藏剧透
          </Button>
        </div>
      ) : (
        <div className="flex items-center justify-between gap-2">
          <p className="text-xs text-muted-foreground">内容含后续剧透</p>
          <Button
            type="button"
            size="sm"
            variant="outline"
            className="h-7 px-2 text-xs"
            aria-expanded={false}
            onClick={() => setRevealed(true)}
          >
            <Eye aria-hidden />
            查看内容
          </Button>
        </div>
      )}
    </div>
  );
}
