import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useId, useMemo, useState } from "react";

import { Button } from "@/components/ui/button";
import { usePlayerStore } from "@/features/player";
import { errorMessage } from "@/lib/format";

import { useAcpProfilesStore } from "../acpProfilesStore";
import { useAcpSettingsStore } from "../acpSettingsStore";
import { useAcpSessionStore } from "../acpSessionStore";
import {
  acpCancel,
  acpClose,
  acpPrompt,
  getAcpStatus,
} from "../api";
import { profilesHintFromStore } from "../defaultAgentProfiles";
import {
  applyAcpEventToTurn,
  createTurn,
  pushNotice,
  type SystemNotice,
} from "../chatTurns";
import { workspaceCwdFromMedia } from "../cwd";
import type { AcpEvent, ChatTurn, PendingPermission, ThinkingLevel } from "../types";
import { useVideoPromptContext } from "../useVideoPromptContext";
import { AgentSettingsPanel } from "./AgentSettingsPanel";
import { ChatComposer } from "./ChatComposer";
import { ChatShell } from "./ChatShell";
import { ChatColumn } from "./ChatShell";
import { ChatTurnList } from "./ChatTurnList";
import { PermissionPrompt } from "./PermissionPrompt";

/** ACP chat — unified column width, turn-based streaming, session id resume. */
export function AcpPanel() {
  const queryClient = useQueryClient();
  const currentFile = usePlayerStore((s) => s.currentFile);
  const thinkingLevel = useAcpSettingsStore((s) => s.thinkingLevel);
  const clientSettings = useAcpSettingsStore((s) => ({
    permissionMode: s.permissionMode,
    thinkingLevel: s.thinkingLevel,
    agentMode: s.agentMode,
  }));
  const savedSession = useAcpSessionStore((s) => s.savedSession);
  const setSavedSession = useAcpSessionStore((s) => s.setSavedSession);
  const clearSavedSession = useAcpSessionStore((s) => s.clearSavedSession);
  const activeProfileId = useAcpProfilesStore((s) => s.activeProfileId);
  const profiles = useAcpProfilesStore((s) => s.profiles);
  const profilesHint = useMemo(
    () => profilesHintFromStore(activeProfileId, profiles),
    [activeProfileId, profiles],
  );

  const statusQuery = useQuery({
    queryKey: ["acp-status", profilesHint],
    queryFn: () => getAcpStatus(profilesHint),
    staleTime: 15_000,
  });

  const idSeq = useState(() => ({ n: 0 }))[0];
  const listKey = useId();
  const [draft, setDraft] = useState("");
  const [turns, setTurns] = useState<ChatTurn[]>([]);
  const [notices, setNotices] = useState<SystemNotice[]>([]);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<string | null>(null);
  const [pendingPermission, setPendingPermission] =
    useState<PendingPermission | null>(null);

  const available = statusQuery.data?.available ?? false;
  const sessionActive = statusQuery.data?.sessionActive ?? false;
  const sessionCwd = workspaceCwdFromMedia(currentFile);
  const videoContext = useVideoPromptContext();

  const pushSystem = (content: string) => {
    setNotices((prev) => pushNotice(prev, idSeq, content));
  };

  const closeMutation = useMutation({
    mutationFn: acpClose,
    onSuccess: async () => {
      clearSavedSession();
      pushSystem("会话已关闭");
      await queryClient.invalidateQueries({ queryKey: ["acp-status"] });
    },
    onError: (error) => pushSystem(errorMessage(error)),
  });

  const handleEvent = (
    event: AcpEvent,
    turnId: string,
    level: ThinkingLevel,
  ) => {
    switch (event.type) {
      case "started":
        setProgress("会话已开始");
        break;
      case "progress":
        setProgress(event.message);
        break;
      case "permissionRequest":
        setPendingPermission({
          requestId: event.requestId,
          toolCallId: event.toolCallId,
          title: event.title,
          options: event.options,
        });
        break;
      case "permissionResolved":
        setPendingPermission(null);
        if (level !== "hidden") {
          pushSystem(`权限：${event.decision}`);
        }
        break;
      case "sessionSaved":
        setSavedSession({
          sessionId: event.sessionId,
          profileId: event.profileId,
          cwd: event.cwd,
        });
        break;
      case "finished":
      case "failed":
      case "agentMessage":
      case "agentThought":
      case "toolCall":
      case "toolCallUpdate":
      case "plan":
        setTurns((prev) =>
          prev.map((t) =>
            t.id === turnId ? applyAcpEventToTurn(t, event, level) : t,
          ),
        );
        if (event.type === "finished") {
          setProgress(null);
          void queryClient.invalidateQueries({ queryKey: ["acp-status"] });
        }
        if (event.type === "failed") {
          setProgress(null);
          void queryClient.invalidateQueries({ queryKey: ["acp-status"] });
        }
        break;
    }
  };

  const runMutation = useMutation({
    mutationFn: async (text: string) => {
      setBusy(true);
      setProgress(null);
      setPendingPermission(null);

      const turn = createTurn(idSeq, text);
      setTurns((prev) => [...prev, turn]);

      try {
        return await acpPrompt(
          text,
          (event: AcpEvent) => handleEvent(event, turn.id, thinkingLevel),
          {
            profileId: activeProfileId,
            cwd: sessionCwd,
            context: videoContext,
            savedSession,
            clientSettings,
            profiles: profilesHint,
          },
        );
      } catch (error) {
        setTurns((prev) =>
          prev.map((t) =>
            t.id === turn.id
              ? applyAcpEventToTurn(
                  t,
                  {
                    type: "failed",
                    code: "Error",
                    message: errorMessage(error),
                  },
                  thinkingLevel,
                )
              : t,
          ),
        );
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
    <ChatShell data-chat-shell={listKey}>
      <AgentSettingsPanel
        status={statusQuery.data}
        loading={statusQuery.isLoading}
        busy={busy}
        onStatusError={pushSystem}
      />

      {(sessionActive || busy) && (
        <ChatColumn className="flex shrink-0 items-center justify-between gap-2 border-b border-border py-1">
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
        </ChatColumn>
      )}

      <ChatTurnList turns={turns} notices={notices} />

      {pendingPermission ? (
        <PermissionPrompt
          pending={pendingPermission}
          onDone={() => setPendingPermission(null)}
        />
      ) : null}

      {progress ? (
        <ChatColumn className="shrink-0 pb-1 text-[11px] text-muted-foreground">
          {progress}
        </ChatColumn>
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
    </ChatShell>
  );
}

