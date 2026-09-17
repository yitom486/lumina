import { History, RefreshCw } from "lucide-react";

import { Button, cn } from "@lumina/ui";

import type { HistoryThreadRow } from "../conversationContext";

type Props = {
  open: boolean;
  rows: HistoryThreadRow[];
  activeSessionId: string | null;
  switchBlocked: boolean;
  loading: boolean;
  onRefresh: () => void;
  onClose: () => void;
  onSelect: (sessionId: string) => void;
};

export function formatConversationTime(updatedAtMs: number): string {
  if (!Number.isFinite(updatedAtMs) || updatedAtMs <= 0) return "";
  return new Intl.DateTimeFormat("zh-CN", {
    month: "numeric",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(updatedAtMs));
}

/**
 * 历史对话 = Agent 侧 `session/list` 原样呈现。本地不存账：
 * 无删除（原生无 delete 方法，假删不如不删）、无本地/Agent 双行制、
 * 无存档态——列表里出现的即原生存在的线程。
 */
export function ChatHistorySheet({
  open,
  rows,
  activeSessionId,
  switchBlocked,
  loading,
  onRefresh,
  onClose,
  onSelect,
}: Props) {
  if (!open) return null;

  return (
    <div className="border-b border-border bg-popover shadow-sm">
      <div className="flex items-center justify-between gap-2 px-1 py-1.5">
        <div className="inline-flex min-w-0 items-center gap-1.5 text-[11px] font-medium">
          <History className="size-3.5 shrink-0" />
          <span className="truncate">历史对话</span>
        </div>
        <div className="flex shrink-0 items-center gap-1">
          <Button
            size="sm"
            variant="ghost"
            className="h-7 px-2 text-[10px]"
            disabled={loading}
            onClick={onRefresh}
          >
            <RefreshCw
              className={cn("size-3", loading && "animate-spin")}
            />
            刷新
          </Button>
          <Button
            size="sm"
            variant="ghost"
            className="h-7 px-2 text-[10px]"
            onClick={onClose}
          >
            关闭
          </Button>
        </div>
      </div>

      {loading && rows.length === 0 ? (
        <p className="px-3 pb-4 pt-1 text-center text-[11px] text-muted-foreground">
          正在加载历史…
        </p>
      ) : rows.length === 0 ? (
        <p className="px-3 pb-4 pt-1 text-center text-[11px] text-muted-foreground">
          暂无历史对话，在本目录开始新对话后会出现在列表中
        </p>
      ) : (
        <ul className="max-h-56 overflow-y-auto pb-1">
          {rows.map((row) => {
            const disabled = switchBlocked;
            const time = formatConversationTime(row.updatedAtMs);
            return (
              <li key={row.sessionId}>
                <div
                  className={cn(
                    "flex items-start gap-2 px-2 py-2 hover:bg-muted/60",
                    row.sessionId === activeSessionId && "bg-muted/40",
                  )}
                >
                  <button
                    type="button"
                    className={cn(
                      "min-w-0 flex-1 text-left",
                      disabled && "cursor-not-allowed opacity-70",
                    )}
                    disabled={disabled}
                    onClick={() => onSelect(row.sessionId)}
                  >
                    <p className="truncate text-[11px] font-medium text-foreground">
                      {row.title}
                    </p>
                    <p className="mt-0.5 text-[10px] text-muted-foreground">
                      {time ? time : `对话 ${row.sessionId.slice(0, 8)}`}
                    </p>
                    {disabled ? (
                      <p className="mt-1 text-[10px] leading-4 text-muted-foreground">
                        正在回答，请稍后再切换对话
                      </p>
                    ) : null}
                  </button>
                </div>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
