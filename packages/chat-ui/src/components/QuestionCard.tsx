import { HelpCircle } from "lucide-react";

import { cn } from "@lumina/ui";

import type {
  AssistantAction,
  QuestionCardBlock,
} from "../assistantBlocks";
import { ActionChip } from "./ActionChip";

type Props = {
  block: QuestionCardBlock;
  onAction?: (action: AssistantAction) => void;
};

export function QuestionCard({ block, onAction }: Props) {
  return (
    <section
      className="space-y-3 rounded-lg border border-border bg-muted/20 p-3"
      aria-labelledby={`question-${block.id}`}
      data-block-kind={block.kind}
    >
      <div className="flex items-start gap-2">
        <HelpCircle className="mt-0.5 size-4 shrink-0 text-primary" aria-hidden />
        <div className="min-w-0 flex-1">
          <h3 id={`question-${block.id}`} className="font-medium text-foreground">
            {block.question}
          </h3>
          {block.anchor ? (
            <p className="mt-1 text-xs text-muted-foreground">
              来源：{formatAnchor(block.anchor)}
            </p>
          ) : null}
        </div>
      </div>
      {block.options.length > 0 ? (
        <div className="flex flex-wrap gap-2">
          {block.options.map((option) => (
            <ActionChip key={option.id} chip={option} onAction={onAction} />
          ))}
        </div>
      ) : (
        <p className={cn("text-xs", "text-muted-foreground")}>可以在对话框中继续追问。</p>
      )}
    </section>
  );
}

function formatAnchor(anchor: QuestionCardBlock["anchor"]): string {
  if (!anchor) return "当前内容";
  if (typeof anchor.startMs === "number") return formatTime(anchor.startMs);
  return `章节 ${anchor.chapterId}`;
}

function formatTime(milliseconds: number): string {
  if (!Number.isFinite(milliseconds) || milliseconds < 0) return "时间轴";
  const totalSeconds = Math.floor(milliseconds / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}:${seconds.toString().padStart(2, "0")}`;
}
