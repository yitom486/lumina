import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useId, useState } from "react";

import { Button } from "@/components/ui/button";
import { usePlayerStore } from "@/features/player";
import { errorMessage } from "@/lib/format";

import { acpCancel, acpClose, acpPrompt, getAcpStatus } from "../api";
import { workspaceCwdFromMedia } from "../cwd";
import type { AcpEvent, ChatMessage } from "../types";
import { AgentSetupBar } from "./AgentSetupBar";
import { ChatComposer } from "./ChatComposer";
import { ChatMessageList } from "./ChatMessageList";

function nextId(prefix: string, seq: { n: number }): string {
  seq.n += 1;
  return `${prefix}-${seq.n}`;
}

/** ACP chat shell. Session cwd defaults to the open media file's directory. */
export function AcpPanel() {
  const queryClient = useQueryClient();
  const currentFile = usePlayerStore((s) => s.currentFile);
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
  const sessionActive = statusQuery.data?.sessionActive ?? false;
  const activeProfileId = statusQuery.data?.activeProfileId ?? "codex";
  const sessionCwd = workspaceCwdFromMedia(currentFile);

  const pushSystem = (content: string) => {
    setMessages((prev) => [
      ...prev,
      { id: nextId("sys", idSeq), role: "system", content },
    ]);
  };

  const closeMutation = useMutation({
    mutationFn: acpClose,
    onSuccess: async () => {
      pushSystem("会话已关闭");
      await queryClient.invalidateQueries({ queryKey: ["acp-status"] });
    },
    onError: (error) => pushSystem(errorMessage(error)),
  });

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
              case "agentThought":
                setProgress(`思考中… ${event.text.slice(0, 80)}`);
                break;
              case "toolCall":
                pushSystem(
                  `工具：${event.title ?? event.toolCallId}${event.status ? `（${event.status}）` : ""}`,
                );
                break;
              case "toolCallUpdate":
                if (event.status) {
                  setProgress(`工具更新：${event.status}`);
                }
                break;
              case "plan":
                pushSystem(`计划\n${event.text}`);
                break;
              case "permissionResolved":
                pushSystem(
                  `权限：${event.decision}${event.toolCallId ? ` · ${event.toolCallId}` : ""}`,
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
                void queryClient.invalidateQueries({ queryKey: ["acp-status"] });
                break;
              case "failed":
                setProgress(null);
                patchAssistant({
                  content: event.message,
                  status: "error",
                });
                void queryClient.invalidateQueries({ queryKey: ["acp-status"] });
                break;
            }
          },
          { profileId: activeProfileId, cwd: sessionCwd },
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
      void queryClient.invalidateQueries({ queryKey: ["acp-status"] });
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

      {(sessionActive || busy) && (
        <div className="flex shrink-0 items-center justify-between gap-2 border-b border-border px-3 py-1">
          <span className="min-w-0 truncate text-[11px] text-muted-foreground">
            {busy ? "回合进行中" : "会话保持中（可继续提问）"}
            {sessionCwd ? ` · ${sessionCwd}` : ""}
          </span>
          <Button
            size="sm"
            variant="ghost"
            className="h-7 shrink-0 text-[11px]"
            disabled={busy || closeMutation.isPending}
            onClick={() => closeMutation.mutate()}
          >
            结束会话
          </Button>
        </div>
      )}

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
