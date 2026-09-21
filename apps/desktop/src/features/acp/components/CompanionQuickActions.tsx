import {
  BookOpenText,
  CircleHelp,
  Compass,
  ListChecks,
  type LucideIcon,
} from "lucide-react";

import { Button } from "@lumina/ui/button";

import type { AcpTaskId } from "../api";

export type CompanionTaskId = AcpTaskId;

type QuickAction = {
  id: CompanionTaskId;
  label: string;
  icon: LucideIcon;
};

const QUICK_ACTIONS: readonly QuickAction[] = [
  { id: "chapter_recap", label: "本段总结", icon: BookOpenText },
  { id: "chapter_outlook", label: "后续看点", icon: Compass },
  { id: "question_candidates", label: "观众问题", icon: CircleHelp },
  { id: "plot_summary", label: "剧情梳理", icon: ListChecks },
];

export const COMPANION_TASK_LABELS = {
  chapter_recap: "本段总结",
  chapter_outlook: "后续看点",
  question_candidates: "观众问题",
  plot_summary: "剧情梳理",
} satisfies Readonly<Record<CompanionTaskId, string>>;

type Props = {
  disabled?: boolean;
  onSelectTask: (taskId: CompanionTaskId) => void;
};

/** Stable task shortcuts; prompt composition belongs to the task layer. */
export function CompanionQuickActions({ disabled, onSelectTask }: Props) {
  return (
    <div
      className="flex flex-wrap items-center gap-1.5"
      role="group"
      aria-label="快捷 AI 操作"
    >
      {QUICK_ACTIONS.map(({ id, label, icon: Icon }) => (
        <Button
          key={id}
          type="button"
          size="sm"
          variant="outline"
          className="h-8 shrink-0 rounded-full border-border/80 bg-background/40 px-3 text-[11px] text-muted-foreground hover:border-accent hover:bg-accent/10 hover:text-foreground [&_svg]:size-3.5"
          disabled={disabled}
          aria-label={label}
          title={label}
          onClick={() => onSelectTask(id)}
        >
          <Icon aria-hidden="true" />
          <span>{label}</span>
        </Button>
      ))}
    </div>
  );
}
