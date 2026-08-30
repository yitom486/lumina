import { useMutation, useQuery } from "@tanstack/react-query";
import { useId, useState } from "react";

import { errorMessage } from "@/lib/format";

import { acpCancel, acpPrompt, getAcpStatus } from "../api";
import type { AcpEvent, ChatMessage } from "../types";
import { AgentSetupBar } from "./AgentSetupBar";
import { ChatComposer } from "./ChatComposer";
import { ChatMessageList } from "./ChatMessageList";

function nextId(prefix: string, seq: { n: number }): string {
  seq.n += 1;
  return `${prefix}-${seq.n}`;
}

/** ACP chat shell. Video context linkage is intentionally deferred. */
export function AcpPanel() {
  const statusQuery = useQuery({
    queryKey: ["acp-status"],
    queryFn: getAcpStatus,
    staleTime: 15_000,
  });

  const idSeq = useState(() => ({ n: 0 }))[0];
  const listKey = useId();
  const [draft, setDraft] = useState("");
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<string | null>(null);

  const available = statusQuery.data?.available ?? false;
  const activeProfileId = statusQuery.data?.activeProfileId ?? "codex";

  const pushSystem = (content: string) => {
    setMessages((prev) => [
      ...prev,
      { id: nextId("sys", idSeq), role: "system", content },
    ]);
  };

  const runMutation = useMutation({
    mutationFn: async (text: string) => {
      setBusy(true);
      setProgress(null);

      const userId = nextId("user", idSeq);
      const assistantId = nextId("asst", idSeq);

      setMessages((prev) => [
        ...prev,
        { id: userId, role: "user", content: text, status: "done" },
        {
          id: assistantId,
          role: "assistant",
          content: "",
          status: "streaming",
        },
      ]);

      const patchAssistant = (
        patch: Partial<Pick<ChatMessage, "content" | "status">>,
      ) => {
        setMessages((prev) =>
          prev.map((m) => (m.id === assistantId ? { ...m, ...patch } : m)),
        );
      };

      try {
        return await acpPrompt(
          text,
          (event: AcpEvent) => {
            switch (event.type) {
              case "started":
                setProgress("会话已开始");
                break;
              case "progress":
                setProgress(event.message);
                break;
              case "agentMessage":
                setMessages((prev) =>
                  prev.map((m) =>
                    m.id === assistantId
                      ? {
                          ...m,
                          content: `${m.content}${event.text}`,
                          status: "streaming",
                        }
                      : m,
                  ),
                );
                break;
              case "finished":
                setProgress(null);
                setMessages((prev) =>
                  prev.map((m) =>
                    m.id === assistantId
                      ? {
                          ...m,
                          content: event.text || m.content,
                          status: "done",
                        }
                      : m,
                  ),
                );
                break;
              case "failed":
                setProgress(null);
                patchAssistant({
                  content: event.message,
                  status: "error",
                });
                break;
            }
          },
          { profileId: activeProfileId },
        );
      } catch (error) {
        patchAssistant({
          content: errorMessage(error),
          status: "error",
        });
        throw error;
      }
    },
    onSettled: () => {
      setBusy(false);
      setProgress(null);
    },
  });

  const send = () => {
    const text = draft.trim();
    if (!text || busy || !available) return;
    setDraft("");
    runMutation.mutate(text);
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col" data-chat-shell={listKey}>
      <AgentSetupBar
        status={statusQuery.data}
        loading={statusQuery.isLoading}
        busy={busy}
        onStatusError={pushSystem}
      />

      <ChatMessageList messages={messages} />

      {progress ? (
        <p className="shrink-0 px-3 pb-1 text-[11px] text-muted-foreground">
          {progress}
        </p>
      ) : null}

      <ChatComposer
        value={draft}
        disabled={!available}
        busy={busy}
        placeholder={
          available
            ? "输入问题（Enter 发送，Shift+Enter 换行）"
            : "请先在设置中配置可用的 Agent"
        }
        onChange={setDraft}
        onSend={send}
        onCancel={() => {
          void acpCancel();
        }}
      />
    </div>
  );
}
