import { useState } from "react";

import { cn } from "@/lib/utils";

import { hasActiveToolActivity, isToolRunning, waitingLabel } from "../activityStatus";
import {
  isToolFailed,
  isToolSucceeded,
  parseToolDetail,
  toolFailureHint,
  toolStatusLabel,
} from "../toolStatus";
import type { ChatActivity } from "../types";
import { ChatWaitingDots } from "./ChatWaitingDots";

type Props = {
  activities: ChatActivity[];
  streaming?: boolean;
  collapsible?: boolean;
  onRequestCollapse?: () => void;
};

export function ChatActivityFeed({
  activities,
  streaming,
  collapsible,
  onRequestCollapse,
}: Props) {
  if (activities.length === 0) return null;

  const live = Boolean(streaming);
  const toolsActive = hasActiveToolActivity(activities);

  return (
    <div
      className={cn(
        "mb-2 space-y-1.5 rounded-md border border-border/60 bg-muted/20 px-2.5 py-2 text-xs",
        live && "chat-activity-live",
      )}
    >
      {collapsible && onRequestCollapse ? (
        <div className="flex justify-end">
          <button
            type="button"
            className="text-[10px] text-muted-foreground hover:text-foreground"
            onClick={onRequestCollapse}
          >
            收起工具轨迹
          </button>
        </div>
      ) : null}
      {activities.map((item) => (
        <ActivityRow
          key={item.id}
          item={item}
          live={live}
          collapsible={Boolean(collapsible && !live && item.kind === "tool")}
        />
      ))}
      {live ? (
        <ChatWaitingDots
          label={toolsActive ? "工具执行中" : waitingLabel(activities)}
          className="pt-0.5"
        />
      ) : null}
    </div>
  );
}

function ActivityRow({
  item,
  live,
  collapsible,
}: {
  item: ChatActivity;
  live: boolean;
  collapsible: boolean;
}) {
  if (item.kind === "thought") {
    return (
      <div className="text-muted-foreground">
        <span className="inline-flex items-center gap-1.5 font-medium text-foreground/80">
          {live ? <span className="chat-tool-orbit" aria-hidden /> : null}
          思考
        </span>
        <p className="mt-0.5 line-clamp-6 whitespace-pre-wrap break-words">
          {item.text?.trim() || "…"}
        </p>
      </div>
    );
  }

  if (item.kind === "plan") {
    return (
      <div>
        <span className="inline-flex items-center gap-1.5 font-medium text-foreground/80">
          {live ? <span className="chat-tool-orbit" aria-hidden /> : null}
          计划
        </span>
        <pre className="mt-0.5 max-h-32 overflow-auto whitespace-pre-wrap break-words font-sans text-muted-foreground">
          {item.text}
        </pre>
      </div>
    );
  }

  return (
    <ToolActivityRow item={item} live={live} collapsible={collapsible} />
  );
}

function ToolActivityRow({
  item,
  live,
  collapsible,
}: {
  item: ChatActivity;
  live: boolean;
  collapsible: boolean;
}) {
  const [detailOpen, setDetailOpen] = useState(false);
  const running = isToolRunning(item.status);
  const failed = isToolFailed(item.status);
  const succeeded = isToolSucceeded(item.status);
  const parsedDetail = parseToolDetail(item.text);
  const failureHint = toolFailureHint(item.status, item.text);
  const summary =
    failureHint ??
    (succeeded ? "执行完成" : parsedDetail?.slice(0, 120) ?? null);
  const canExpand =
    collapsible &&
    Boolean(parsedDetail && parsedDetail.length > 0 && (failed || succeeded));

  return (
    <div
      className={cn(
        "flex items-start gap-2 rounded-md px-1 py-1",
        live && running && "chat-tool-row-active",
        failed && "bg-destructive/5",
        succeeded && !failed && "bg-emerald-500/5",
      )}
    >
      {live && running ? (
        <div className="chat-tool-scan-line" aria-hidden />
      ) : null}
      <span className="relative mt-0.5 shrink-0">
        {live && running ? (
          <span className="chat-tool-orbit" aria-hidden />
        ) : (
          <span
            className={cn(
              "block size-2 rounded-full",
              toolStatusColor(item.status, live && running),
            )}
          />
        )}
      </span>
      <div className="min-w-0 flex-1">
        <div className="flex items-start justify-between gap-2">
          <div className="min-w-0">
            <p className="font-medium text-foreground/90">
              {item.title ?? item.toolCallId ?? "工具"}
            </p>
            {item.status ? (
              <p
                className={cn(
                  "text-[10px]",
                  failed && "text-destructive",
                  succeeded && "text-emerald-600 dark:text-emerald-400",
                  !failed && !succeeded && "text-muted-foreground",
                )}
              >
                {toolStatusLabel(item.status)}
              </p>
            ) : null}
          </div>
          {canExpand ? (
            <button
              type="button"
              className="shrink-0 text-[10px] text-muted-foreground hover:text-foreground"
              onClick={() => setDetailOpen((open) => !open)}
            >
              {detailOpen ? "收起" : failed ? "查看原因" : "查看输出"}
            </button>
          ) : null}
        </div>

        {failed && failureHint ? (
          <p className="mt-0.5 whitespace-pre-wrap break-words text-[10px] leading-relaxed text-destructive/90">
            {failureHint}
          </p>
        ) : summary && (!collapsible || detailOpen || live || running) ? (
          <p
            className={cn(
              "mt-0.5 whitespace-pre-wrap break-words text-[10px] leading-relaxed text-muted-foreground",
              collapsible && !detailOpen && !live && canExpand && "line-clamp-2",
            )}
          >
            {summary}
          </p>
        ) : null}

        {detailOpen && parsedDetail ? (
          <pre className="mt-1 max-h-40 overflow-auto whitespace-pre-wrap break-words rounded border border-border/60 bg-background/80 p-2 text-[10px] leading-relaxed text-muted-foreground">
            {parsedDetail}
          </pre>
        ) : null}

        {!collapsible && item.text && !failed && !detailOpen ? (
          <p className="mt-0.5 line-clamp-4 whitespace-pre-wrap break-words text-[10px] leading-relaxed text-muted-foreground">
            {parsedDetail ?? item.text}
          </p>
        ) : null}
      </div>
    </div>
  );
}

function toolStatusColor(status?: string, running?: boolean): string {
  if (running) return "bg-amber-400 animate-pulse";
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
