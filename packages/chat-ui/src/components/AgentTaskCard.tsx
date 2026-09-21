import { Check, CircleAlert, CircleDot, Clock3, X } from "lucide-react";

import { cn } from "@lumina/ui";

import type {
  AgentTaskStatus,
  AgentTaskStatusBlock,
} from "../assistantBlocks";

type Props = {
  block: AgentTaskStatusBlock;
};

const STATUS_LABELS: Record<AgentTaskStatus, string> = {
  queued: "排队中",
  running: "处理中",
  succeeded: "已完成",
  failed: "处理失败",
  cancelled: "已取消",
};

export function AgentTaskCard({ block }: Props) {
  return (
    <section
      className="space-y-2 rounded-lg border border-border bg-muted/20 p-3"
      aria-label={`任务状态：${block.title}`}
      data-block-kind={block.kind}
      data-task-id={block.taskId}
    >
      <div className="flex items-start gap-2">
        <StatusIcon status={block.status} />
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <h3 className="font-medium text-foreground">{block.title}</h3>
            <span className={statusClassName(block.status)}>
              {STATUS_LABELS[block.status]}
            </span>
          </div>
          {block.message ? (
            <p className="mt-1 whitespace-pre-wrap break-words text-xs leading-relaxed text-muted-foreground">
              {block.message}
            </p>
          ) : null}
        </div>
      </div>
      {typeof block.progress === "number" ? (
        <div className="space-y-1">
          <div className="flex justify-between text-[11px] text-muted-foreground">
            <span>进度</span>
            <span>{Math.round(block.progress)}%</span>
          </div>
          <div
            className="h-1.5 overflow-hidden rounded-full bg-muted"
            role="progressbar"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={block.progress}
          >
            <div
              className="h-full rounded-full bg-primary transition-[width]"
              style={{ width: `${block.progress}%` }}
            />
          </div>
        </div>
      ) : null}
    </section>
  );
}

function StatusIcon({ status }: { status: AgentTaskStatus }) {
  switch (status) {
    case "queued":
      return <Clock3 className="mt-0.5 size-4 shrink-0 text-muted-foreground" aria-hidden />;
    case "running":
      return <CircleDot className="mt-0.5 size-4 shrink-0 text-primary" aria-hidden />;
    case "succeeded":
      return <Check className="mt-0.5 size-4 shrink-0 text-primary" aria-hidden />;
    case "failed":
      return <CircleAlert className="mt-0.5 size-4 shrink-0 text-destructive" aria-hidden />;
    case "cancelled":
      return <X className="mt-0.5 size-4 shrink-0 text-muted-foreground" aria-hidden />;
  }
}

function statusClassName(status: AgentTaskStatus): string {
  return cn(
    "rounded-full px-2 py-0.5 text-[11px] font-medium",
    status === "running" && "bg-primary/10 text-primary",
    status === "succeeded" && "bg-primary/10 text-primary",
    status === "failed" && "bg-destructive/10 text-destructive",
    (status === "queued" || status === "cancelled") &&
      "bg-muted text-muted-foreground",
  );
}
