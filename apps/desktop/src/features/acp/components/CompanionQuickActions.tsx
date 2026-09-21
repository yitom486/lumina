import {
  BookOpenText,
  CircleHelp,
  Compass,
  ListChecks,
  type LucideIcon,
} from "lucide-react";

import { Button } from "@lumina/ui/button";

import type { AcpTaskId } from "../api";
import { SHORTCUT_TASK_LABELS } from "../shortcutOutput";

export type CompanionTaskId = AcpTaskId;

type QuickAction = {
  id: CompanionTaskId;
  label: string;
  icon: LucideIcon;
};

const QUICK_ACTIONS: readonly QuickAction[] = [
  { id: "chapter_recap", label: SHORTCUT_TASK_LABELS.chapter_recap, icon: BookOpenText },
  { id: "chapter_outlook", label: SHORTCUT_TASK_LABELS.chapter_outlook, icon: Compass },
  {
    id: "question_candidates",
    label: SHORTCUT_TASK_LABELS.question_candidates,
    icon: CircleHelp,
  },
  { id: "plot_summary", label: SHORTCUT_TASK_LABELS.plot_summary, icon: ListChecks },
];

export const COMPANION_TASK_LABELS = SHORTCUT_TASK_LABELS;

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
