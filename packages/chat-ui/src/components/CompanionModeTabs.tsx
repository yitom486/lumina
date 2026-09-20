import { MessageCircle, Sparkles } from "lucide-react";

import { cn } from "@lumina/ui";

export type CompanionMode = "watch-feed" | "chat";

type Props = {
  value: CompanionMode;
  onChange: (value: CompanionMode) => void;
};

const MODES: ReadonlyArray<{
  value: CompanionMode;
  label: string;
  description: string;
  Icon: typeof Sparkles;
}> = [
  {
    value: "watch-feed",
    label: "AI 观剧流",
    description: "围绕当前观看位置整理洞察",
    Icon: Sparkles,
  },
  {
    value: "chat",
    label: "自由聊天",
    description: "与 Agent 进行完整对话",
    Icon: MessageCircle,
  },
];

/** Shared companion navigation; it only switches projections, never sessions. */
export function CompanionModeTabs({ value, onChange }: Props) {
  return (
    <div
      className="grid grid-cols-2 gap-1 rounded-lg border border-border bg-muted/30 p-1"
      role="tablist"
      aria-label="AI 工作区"
    >
      {MODES.map(({ value: mode, label, description, Icon }) => {
        const active = value === mode;
        return (
          <button
            key={mode}
            type="button"
            role="tab"
            aria-selected={active}
            aria-controls={`companion-panel-${mode}`}
            className={cn(
              "flex min-w-0 items-center gap-2 rounded-md px-2.5 py-2 text-left transition-colors",
              "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
              active
                ? "bg-card text-foreground shadow-sm"
                : "text-muted-foreground hover:bg-card/70 hover:text-foreground",
            )}
            onClick={() => onChange(mode)}
          >
            <Icon className="size-3.5 shrink-0" aria-hidden />
            <span className="min-w-0">
              <span className="block truncate text-xs font-medium">{label}</span>
              <span className="block truncate text-[10px] text-muted-foreground">
                {description}
              </span>
            </span>
          </button>
        );
      })}
    </div>
  );
}
