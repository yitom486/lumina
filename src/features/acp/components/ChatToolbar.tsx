import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

import type { AcpConnectionState } from "../types";
import { ChatColumn } from "./ChatShell";

type Props = {
  agentLabel: string;
  connectionState: AcpConnectionState;
  statusLine?: string | null;
  statusError?: string | null;
  loading?: boolean;
  sessionActive?: boolean;
  busy?: boolean;
  historyItems: { id: string; label: string }[];
  onNewChat: () => void;
  onPickHistory: (id: string) => void;
  onEndSession?: () => void;
  onReconnect?: () => void;
};

const CONNECTION_LABEL: Record<AcpConnectionState, string> = {
  unavailable: "未配置",
  idle: "未连接",
  connecting: "连接中",
  connected: "已连接",
  error: "连接失败",
};

/** Session controls — matches common chat apps (new chat / history on top). */
export function ChatToolbar({
  agentLabel,
  connectionState,
  statusLine,
  statusError,
  loading,
  sessionActive,
  busy,
  historyItems,
  onNewChat,
  onPickHistory,
  onEndSession,
  onReconnect,
}: Props) {
  return (
    <ChatColumn className="shrink-0 space-y-1.5 border-b border-border py-2">
      <div className="flex items-center gap-1.5">
        <Button
          size="sm"
          variant="outline"
          className="h-7 shrink-0 px-2 text-[11px]"
          disabled={busy}
          onClick={onNewChat}
        >
          新建对话
        </Button>

        <details className="relative shrink-0">
          <summary
            className={cn(
              "flex h-7 cursor-pointer list-none items-center rounded-md border border-border px-2 text-[11px] text-muted-foreground",
              "hover:bg-muted [&::-webkit-details-marker]:hidden",
              historyItems.length === 0 && "hidden",
            )}
          >
            本会话
          </summary>
          {historyItems.length > 0 ? (
            <ul className="absolute left-0 z-20 mt-1 max-h-40 w-52 overflow-y-auto rounded-md border border-border bg-popover py-1 shadow-md">
              {historyItems.map((item) => (
                <li key={item.id}>
                  <button
                    type="button"
                    className="block w-full truncate px-2 py-1.5 text-left text-[11px] hover:bg-muted"
                    onClick={() => onPickHistory(item.id)}
                  >
                    {item.label}
                  </button>
                </li>
              ))}
            </ul>
          ) : null}
        </details>

        <span
          className={cn(
            "ml-auto inline-flex min-w-0 shrink items-center gap-1 rounded-full px-2 py-0.5 text-[10px]",
            connectionState === "connected" &&
              "bg-emerald-500/15 text-emerald-700 dark:text-emerald-400",
            connectionState === "connecting" &&
              "bg-amber-500/15 text-amber-700 dark:text-amber-400",
            connectionState === "error" &&
              "bg-destructive/15 text-destructive",
            (connectionState === "idle" ||
              connectionState === "unavailable") &&
              "bg-muted text-muted-foreground",
          )}
          title={CONNECTION_LABEL[connectionState]}
        >
          <span
            className={cn(
              "h-1.5 w-1.5 shrink-0 rounded-full",
              connectionState === "connected" && "bg-emerald-500",
              connectionState === "connecting" &&
                "animate-pulse bg-amber-500",
              connectionState === "error" && "bg-destructive",
              (connectionState === "idle" ||
                connectionState === "unavailable") &&
                "bg-muted-foreground/50",
            )}
          />
          <span className="truncate">{CONNECTION_LABEL[connectionState]}</span>
        </span>

        <span className="min-w-0 max-w-[8rem] truncate text-[11px] text-muted-foreground">
          {loading ? "正在检测…" : agentLabel}
        </span>

        {connectionState === "error" && onReconnect ? (
          <Button
            size="sm"
            variant="outline"
            className="h-7 shrink-0 px-2 text-[11px]"
            disabled={busy || loading}
            onClick={onReconnect}
          >
            重连
          </Button>
        ) : null}

        {sessionActive && !busy ? (
          <Button
            size="sm"
            variant="ghost"
            className="h-7 shrink-0 px-2 text-[11px]"
            disabled={busy}
            onClick={onEndSession}
          >
            结束会话
          </Button>
        ) : null}
      </div>

      {statusError ? (
        <p className="truncate text-[10px] text-destructive">{statusError}</p>
      ) : statusLine ? (
        <p className="truncate text-[10px] text-muted-foreground">{statusLine}</p>
      ) : null}
    </ChatColumn>
  );
}
