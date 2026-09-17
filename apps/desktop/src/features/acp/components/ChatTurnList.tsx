import { useEffect, useRef } from "react";

import type { ChatTurn } from "../types";
import { ChatColumn } from "@lumina/chat-ui/components/ChatShell";
import { ChatTurnView } from "./ChatTurnView";

type Props = {
  turns: ChatTurn[];
  notices: { id: string; content: string }[];
  emptyHint?: string;
  /**
   * 为 true 才跟随到底部（现问现答的流式过程）。
   * 打开历史时调用方置 false 并自行滚到顶部：读旧对话从头看，
   * 不能每次落定都被拽回尾部。
   */
  followEnd: boolean;
  annotationWorkspace?: string | null;
  onDismissAnnotation?: (turnId: string) => void;
  onSaveAnnotation?: (turnId: string) => void;
};

export function ChatTurnList({
  turns,
  notices,
  emptyHint = "向 AI Agent 提问。打开视频后会附带播放上下文。",
  followEnd,
  annotationWorkspace,
  onDismissAnnotation,
  onSaveAnnotation,
}: Props) {
  const endRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (followEnd) endRef.current?.scrollIntoView({ behavior: "auto", block: "end" });
  }, [turns, notices, followEnd]);

  if (turns.length === 0 && notices.length === 0) {
    return (
      <ChatColumn className="flex min-h-0 flex-1 items-center justify-center py-8 text-center text-xs text-muted-foreground">
        {emptyHint}
      </ChatColumn>
    );
  }

  // 操作反馈跟在对话尾部（时间顺序）。会话出身（恢复/新建）是出生证明，
  // 由调用方钉在列表顶部单槽展示，不进这里、不堆积。
  return (
    <ChatColumn className="min-h-0 space-y-4 py-3">
      {turns.map((turn) => (
        <ChatTurnView
          key={turn.id}
          turn={turn}
          annotationWorkspace={annotationWorkspace}
          onDismissAnnotation={onDismissAnnotation}
          onSaveAnnotation={onSaveAnnotation}
        />
      ))}
      {notices.map((n) => (
        <p key={n.id} className="text-center text-[11px] text-muted-foreground">
          {n.content}
        </p>
      ))}
      <div ref={endRef} />
    </ChatColumn>
  );
}
