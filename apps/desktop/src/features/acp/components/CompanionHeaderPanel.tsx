import { Minus, Plus, Sparkles } from "lucide-react";
import { useState } from "react";

import { Button } from "@lumina/ui/button";
import type { AssistantAction } from "@lumina/chat-ui/assistantBlocks";
import { ChatColumn } from "@lumina/chat-ui/components/ChatShell";
import {
  CompanionModeTabs,
  type CompanionMode,
} from "@lumina/chat-ui/components/CompanionModeTabs";

import type { CompanionTaskId } from "./CompanionQuickActions";
import { WatchFeedView } from "./WatchFeedView";

// v2：上一版默认收起阶段可能已写入 "0"，换 key 让“默认展开”对老用户也生效。
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
  mode: CompanionMode;
  onModeChange: (mode: CompanionMode) => void;
  onSelectTask: (taskId: CompanionTaskId) => void;
  quickActionsDisabled?: boolean;
  onAssistantAction?: (action: AssistantAction) => void;
};

/**
 * 置顶的观剧助手手风琴：模式切换 + 观剧流上下文 + 快捷操作收进一个
 * 可展开/收缩的块，默认展开为完整面板，右端 - 收起为单行，不属于下方聊天滚动流。
 */
export function CompanionHeaderPanel({
  mode,
  onModeChange,
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
      <div className="flex items-center gap-2">
        <Sparkles className="size-3.5 shrink-0 text-primary" aria-hidden />
        <div className="min-w-0 flex-1">
          <p className="truncate text-xs font-medium text-foreground">
            {mode === "watch-feed" ? "AI 观剧流" : "自由聊天"}
          </p>
          <p className="truncate text-[10px] text-muted-foreground">
            {mode === "watch-feed"
              ? "观剧上下文与快捷操作"
              : "与 Agent 进行完整对话"}
          </p>
        </div>
        <Button
          type="button"
          size="icon"
          variant="ghost"
          className="size-7 shrink-0"
          aria-label={expanded ? "收起观剧助手" : "展开观剧助手"}
          aria-expanded={expanded}
          aria-controls="companion-header-body"
          title={expanded ? "收起" : "展开"}
          onClick={toggle}
        >
          {expanded ? (
            <Minus className="size-3.5" aria-hidden />
          ) : (
            <Plus className="size-3.5" aria-hidden />
          )}
        </Button>
      </div>

      {expanded ? (
        <div id="companion-header-body" className="space-y-2 pt-2">
          <CompanionModeTabs value={mode} onChange={onModeChange} />
          {mode === "watch-feed" ? (
            <WatchFeedView
              onSelectTask={onSelectTask}
              onAssistantAction={onAssistantAction}
              quickActionsDisabled={quickActionsDisabled}
            />
          ) : null}
        </div>
      ) : null}
    </ChatColumn>
  );
}
