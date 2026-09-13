import { History, Plus, StickyNote } from "lucide-react";
import { useRef } from "react";

import { Button } from "@/components/ui/button";
import { QuickNoteDialog } from "@/features/notes/components/QuickNoteDialog";
import { cn } from "@/lib/utils";

import type { AcpConnectionState } from "../types";
import { ChatColumn } from "./ChatShell";

type Props = {
  agentLabel: string;
  chatTitle?: string | null;
  connectionState: AcpConnectionState;
  statusLine?: string | null;
  statusError?: string | null;
  loading?: boolean;
  busy?: boolean;
  historyCount?: number;
  quickNoteDisabled?: boolean;
  quickNoteOpen?: boolean;
  onQuickNoteOpenChange?: (open: boolean) => void;
  onQuickNoteSaved?: () => void;
  onNewChat: () => void;
  onOpenHistory?: () => void;
  onReconnect?: () => void;
};

const CONNECTION_LABEL: Record<AcpConnectionState, string> = {
  unavailable: "未配置",
  idle: "未连接",
  connecting: "连接中",
  connected: "已连接",
  error: "连接失败",
};

/** Header — status, title preview, history / new chat actions. */
export function ChatToolbar({
  agentLabel,
  chatTitle,
  connectionState,
  statusLine,
  statusError,
  loading,
  busy,
  historyCount = 0,
  quickNoteDisabled,
  quickNoteOpen = false,
  onQuickNoteOpenChange,
  onQuickNoteSaved,
  onNewChat,
  onOpenHistory,
  onReconnect,
}: Props) {
  const quickNoteAnchorRef = useRef<HTMLDivElement>(null);

  return (
    <ChatColumn className="shrink-0 space-y-1 border-b border-border py-2">
      <div className="relative">
      <div className="flex items-start gap-2">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="text-[12px] font-medium text-foreground">
              {agentLabel}
            </span>
            <span
              className={cn(
                "inline-flex min-w-0 items-center gap-1 rounded-full px-2 py-0.5 text-[10px]",
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
              {CONNECTION_LABEL[connectionState]}
            </span>
          </div>
          {chatTitle ? (
            <p className="mt-1 truncate text-[11px] text-muted-foreground">
              {chatTitle}
            </p>
          ) : null}
        </div>

        <div className="flex shrink-0 items-center gap-1">
          {onQuickNoteOpenChange ? (
            <div ref={quickNoteAnchorRef}>
              <Button
                size="icon"
                variant="ghost"
                className="size-7"
                disabled={busy || quickNoteDisabled}
                aria-label="快速写批注"
                aria-expanded={quickNoteOpen}
                title="快速写批注"
                onClick={() => onQuickNoteOpenChange(!quickNoteOpen)}
              >
                <StickyNote className="size-3.5" />
              </Button>
            </div>
          ) : null}
          {onOpenHistory ? (
            <Button
              size="icon"
              variant="ghost"
              className="relative size-7"
              disabled={busy}
              aria-label="历史对话"
              title="历史对话"
              onClick={onOpenHistory}
            >
              <History className="size-3.5" />
              {historyCount > 0 ? (
                <span className="absolute -right-0.5 -top-0.5 flex size-3.5 items-center justify-center rounded-full bg-primary text-[8px] text-primary-foreground">
                  {historyCount > 9 ? "9+" : historyCount}
                </span>
              ) : null}
            </Button>
          ) : null}
          <Button
            size="icon"
            variant="ghost"
            className="size-7"
            disabled={busy}
            aria-label="新建对话"
            title="新建对话"
            onClick={onNewChat}
          >
            <Plus className="size-3.5" />
          </Button>
          {connectionState === "error" || connectionState === "idle" ? (
            onReconnect ? (
              <Button
                size="sm"
                variant="outline"
                className="h-7 px-2 text-[11px]"
                disabled={busy || loading}
                onClick={onReconnect}
              >
                重连
              </Button>
            ) : null
          ) : null}
        </div>
      </div>

      {onQuickNoteOpenChange ? (
        <QuickNoteDialog
          open={quickNoteOpen}
          onOpenChange={onQuickNoteOpenChange}
          onSaved={onQuickNoteSaved}
          anchorRef={quickNoteAnchorRef}
        />
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
