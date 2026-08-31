import { cn } from "@/lib/utils";

import type { ChatActivity } from "../types";

type Props = {
  activities: ChatActivity[];
  streaming?: boolean;
};

export function ChatActivityFeed({ activities, streaming }: Props) {
  if (activities.length === 0) return null;

  return (
    <div className="mb-2 space-y-1.5 rounded-md border border-border/60 bg-muted/20 px-2.5 py-2 text-xs">
      {activities.map((item) => (
        <ActivityRow key={item.id} item={item} />
      ))}
      {streaming ? (
        <p className="text-[10px] text-muted-foreground">处理中…</p>
      ) : null}
    </div>
  );
}

function ActivityRow({ item }: { item: ChatActivity }) {
  if (item.kind === "thought") {
    return (
      <div className="text-muted-foreground">
        <span className="font-medium text-foreground/80">思考</span>
        <p className="mt-0.5 line-clamp-6 whitespace-pre-wrap break-words">
          {item.text?.trim() || "…"}
        </p>
      </div>
    );
  }

  if (item.kind === "plan") {
    return (
      <div>
        <span className="font-medium text-foreground/80">计划</span>
        <pre className="mt-0.5 max-h-32 overflow-auto whitespace-pre-wrap break-words font-sans text-muted-foreground">
          {item.text}
        </pre>
      </div>
    );
  }

  return (
    <div className="flex items-start gap-2">
      <span
        className={cn(
          "mt-0.5 size-1.5 shrink-0 rounded-full",
          toolStatusColor(item.status),
        )}
      />
      <div className="min-w-0 flex-1">
        <p className="font-medium text-foreground/90">
          {item.title ?? item.toolCallId ?? "工具"}
        </p>
        {item.status ? (
          <p className="text-[10px] text-muted-foreground">{item.status}</p>
        ) : null}
      </div>
    </div>
  );
}

function toolStatusColor(status?: string): string {
  switch (status) {
    case "completed":
    case "success":
      return "bg-emerald-500";
    case "failed":
    case "error":
      return "bg-destructive";
    case "in_progress":
    case "running":
      return "bg-amber-500 animate-pulse";
    default:
      return "bg-muted-foreground/60";
  }
}
