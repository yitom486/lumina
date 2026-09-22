import { Minus, Plus, Sparkles } from "lucide-react";
import { useState } from "react";

import type { AssistantAction } from "@lumina/chat-ui/assistantBlocks";
import { ChatColumn } from "@lumina/chat-ui/components/ChatShell";
import { cn } from "@lumina/ui/utils";

import type { CompanionTaskId } from "./CompanionQuickActions";
import { WatchFeedView } from "./WatchFeedView";

const STORAGE_KEY = "lumina-companion-header-expanded-v2";

function readInitialExpanded(): boolean {
  try {
    // 手风琴默认展开；只有用户明确收起过才保持收起。
    return localStorage.getItem(STORAGE_KEY) !== "0";
  } catch {
    return true;
  }
}

type Props = {
  onSelectTask: (taskId: CompanionTaskId) => void;
  quickActionsDisabled?: boolean;
  onAssistantAction?: (action: AssistantAction) => void;
};

/**
 * 置顶的观剧助手手风琴：观剧流上下文 + 快捷操作收进一个
 * 可展开/收缩的块，默认展开为完整面板，右端 - 收起为单行，不属于下方聊天滚动流。
 * 自由聊天就是直接在输入框提问，不再有单独的模式切换。
 */
export function CompanionHeaderPanel({
  onSelectTask,
  quickActionsDisabled,
  onAssistantAction,
}: Props) {
  const [expanded, setExpanded] = useState(readInitialExpanded);

  const toggle = () => {
    setExpanded((value) => {
      const next = !value;
      try {
        localStorage.setItem(STORAGE_KEY, next ? "1" : "0");
      } catch {
        // 忽略持久化失败，保持本次内存状态。
      }
      return next;
    });
  };

  return (
    <ChatColumn
      className="shrink-0 border-b border-border py-2"
      data-companion-header
    >
      {/* 整行即开关：点标题、副标题、图标任意处都展开/收起，右端 +/- 只是视觉指示。 */}
      <button
        type="button"
        aria-expanded={expanded}
        aria-controls="companion-header-body"
        aria-label={expanded ? "收起观剧助手" : "展开观剧助手"}
        title={expanded ? "收起" : "展开"}
        onClick={toggle}
        className={cn(
          "-mx-1 flex w-[calc(100%+0.5rem)] items-center gap-2 rounded-md px-1 py-0.5 text-left",
          "transition-colors hover:bg-muted/40",
          "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
        )}
      >
        <Sparkles className="size-3.5 shrink-0 text-primary" aria-hidden />
        <span className="min-w-0 flex-1">
          <span className="block truncate text-xs font-medium text-foreground">
            AI 观剧流
          </span>
          <span className="block truncate text-[10px] text-muted-foreground">
            观剧上下文与快捷操作
          </span>
        </span>
        {expanded ? (
          <Minus className="size-3.5 shrink-0 text-muted-foreground" aria-hidden />
        ) : (
          <Plus className="size-3.5 shrink-0 text-muted-foreground" aria-hidden />
        )}
      </button>

      {expanded ? (
        <div id="companion-header-body" className="space-y-2 pt-2">
          <WatchFeedView
            onSelectTask={onSelectTask}
            onAssistantAction={onAssistantAction}
            quickActionsDisabled={quickActionsDisabled}
          />
        </div>
      ) : null}
    </ChatColumn>
  );
}
