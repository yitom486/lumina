import { useEffect, useRef } from "react";

import { cn } from "@/lib/utils";

import type { ChatMessage } from "../types";

type Props = {
  messages: ChatMessage[];
  emptyHint?: string;
};

export function ChatMessageList({
  messages,
  emptyHint = "向 AI Agent 提问。打开视频后会自动附带播放进度、章节、字幕与笔记上下文。",
}: Props) {
  const endRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    endRef.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  }, [messages]);

  if (messages.length === 0) {
    return (
      <div className="flex min-h-0 flex-1 items-center justify-center px-4 text-center text-xs text-muted-foreground">
        {emptyHint}
      </div>
    );
  }

  return (
    <div className="min-h-0 flex-1 space-y-2.5 overflow-auto px-3 py-2">
      {messages.map((msg) => (
        <ChatBubble key={msg.id} message={msg} />
      ))}
      <div ref={endRef} />
    </div>
  );
}

function ChatBubble({ message }: { message: ChatMessage }) {
  if (message.role === "system") {
    return (
      <div className="text-center text-[11px] text-muted-foreground">
        {message.content}
      </div>
    );
  }

  const isUser = message.role === "user";
  const isError = message.status === "error";
  const isStreaming = message.status === "streaming";

  return (
    <div className={cn("flex", isUser ? "justify-end" : "justify-start")}>
      <div
        className={cn(
          "max-w-[92%] rounded-lg px-2.5 py-1.5 text-sm whitespace-pre-wrap break-words",
          isUser && "bg-accent text-accent-foreground",
          !isUser && !isError && "bg-muted/50 text-foreground",
          isError && "bg-destructive/10 text-destructive",
        )}
      >
        {message.content || (isStreaming ? "…" : "")}
        {isStreaming ? (
          <span className="ml-0.5 inline-block animate-pulse text-muted-foreground">
            ▍
          </span>
        ) : null}
      </div>
    </div>
  );
}
