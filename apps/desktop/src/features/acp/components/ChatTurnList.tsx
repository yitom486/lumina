import { useEffect, useRef } from "react";

import type { ChatTurn } from "../types";
import { ChatColumn } from "./ChatShell";
import { ChatTurnView } from "./ChatTurnView";

type Props = {
  turns: ChatTurn[];
  notices: { id: string; content: string }[];
  emptyHint?: string;
  annotationWorkspace?: string | null;
  onDismissAnnotation?: (turnId: string) => void;
  onSaveAnnotation?: (turnId: string, proposalId?: string) => void;
};

export function ChatTurnList({
  turns,
  notices,
  emptyHint = "向 AI Agent 提问。打开视频后会附带播放上下文。",
  annotationWorkspace,
  onDismissAnnotation,
  onSaveAnnotation,
}: Props) {
  const endRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    endRef.current?.scrollIntoView({ behavior: "auto", block: "end" });
  }, [turns, notices]);

  if (turns.length === 0 && notices.length === 0) {
    return (
      <ChatColumn className="flex min-h-0 flex-1 items-center justify-center py-8 text-center text-xs text-muted-foreground">
        {emptyHint}
      </ChatColumn>
    );
  }

  return (
    <ChatColumn className="min-h-0 space-y-4 py-3">
      {notices.map((n) => (
        <p key={n.id} className="text-center text-[11px] text-muted-foreground">
          {n.content}
        </p>
      ))}
      {turns.map((turn) => (
        <ChatTurnView
          key={turn.id}
          turn={turn}
          annotationWorkspace={annotationWorkspace}
          onDismissAnnotation={onDismissAnnotation}
          onSaveAnnotation={onSaveAnnotation}
        />
      ))}
      <div ref={endRef} />
    </ChatColumn>
  );
}
