import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useId, useMemo, useRef, useState } from "react";

import { usePlayerStore } from "@/features/player";
import { errorMessage } from "@/lib/format";

import "../chat-motion.css";

import { useAcpProfilesStore } from "../acpProfilesStore";
import { useAcpSettingsStore } from "../acpSettingsStore";
import { useAcpSessionStore } from "../acpSessionStore";
import {
  acpCancel,
  acpClose,
  acpConnect,
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
import { profilesSignature } from "../profilesSignature";
import type {
  AcpConnectionState,
  AcpEvent,
  ChatTurn,
  PendingPermission,
  ThinkingLevel,
} from "../types";
import { useVideoPromptContext } from "../useVideoPromptContext";
import { AgentSettingsPanel } from "./AgentSettingsPanel";
import { ChatComposer } from "./ChatComposer";
import { ChatShell } from "./ChatShell";
import { ChatColumn } from "./ChatShell";
import { ChatToolbar } from "./ChatToolbar";
import { ChatTurnList } from "./ChatTurnList";
import { PermissionPrompt } from "./PermissionPrompt";

/** Kept mounted in ChatDock after first open; hide ≠ unmount. */
export function AcpPanel() {
  const queryClient = useQueryClient();
  const currentFile = usePlayerStore((s) => s.currentFile);
  const thinkingLevel = useAcpSettingsStore((s) => s.thinkingLevel);
  const activeProfileId = useAcpProfilesStore((s) => s.activeProfileId);
  const profilesSig = useAcpProfilesStore((s) => profilesSignature(s.profiles));
  const hasSavedSession = useAcpSessionStore((s) => s.savedSession !== null);
  const setSavedSession = useAcpSessionStore((s) => s.setSavedSession);
  const clearSavedSession = useAcpSessionStore((s) => s.clearSavedSession);

  const statusQuery = useQuery({
    queryKey: ["acp-status", activeProfileId, profilesSig],
    queryFn: () => {
      const state = useAcpProfilesStore.getState();
      return getAcpStatus(
        profilesHintFromStore(state.activeProfileId, state.profiles),
      );
    },
    staleTime: 15_000,
  });

  const idSeq = useState(() => ({ n: 0 }))[0];
  const listKey = useId();
  const turnListRef = useRef<HTMLDivElement | null>(null);
  const [draft, setDraft] = useState("");
  const [turns, setTurns] = useState<ChatTurn[]>([]);
  const [notices, setNotices] = useState<SystemNotice[]>([]);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<string | null>(null);
  const [connectionState, setConnectionState] =
    useState<AcpConnectionState>("idle");
  const [connectAttempt, setConnectAttempt] = useState(0);
  const skipAutoConnectRef = useRef(false);
  const prevConnectKeyRef = useRef<string | null>(null);
  const [pendingPermission, setPendingPermission] =
    useState<PendingPermission | null>(null);

  const available = statusQuery.data?.available ?? false;
  const sessionActive = statusQuery.data?.sessionActive ?? false;
  const sessionCwd = workspaceCwdFromMedia(currentFile);
  const connectKey = `${activeProfileId}:${profilesSig}:${sessionCwd ?? ""}`;
  const videoContext = useVideoPromptContext();

  const activeProfile = statusQuery.data?.profiles.find(
    (profile) => profile.id === activeProfileId,
  );
  const agentLabel = activeProfile?.name ?? "Agent";

  const historyItems = useMemo(
    () =>
      turns
        .filter((turn) => turn.userText.trim() || turn.answer.trim())
        .map((turn) => ({
          id: turn.id,
          label:
            turn.userText.trim().slice(0, 48) ||
            turn.answer.trim().slice(0, 48),
        })),
    [turns],
  );

  const isBlankChat = turns.length === 0 && notices.length === 0;

  const pushSystem = (content: string) => {
    setNotices((prev) => pushNotice(prev, idSeq, content));
  };

  const closeMutation = useMutation({
    mutationFn: acpClose,
    onSuccess: async () => {
      clearSavedSession();
      skipAutoConnectRef.current = true;
      setConnectionState("idle");
      pushSystem("会话已关闭");
      await queryClient.invalidateQueries({ queryKey: ["acp-status"] });
    },
    onError: (error) => pushSystem(errorMessage(error)),
  });

  useEffect(() => {
    if (prevConnectKeyRef.current !== connectKey) {
      prevConnectKeyRef.current = connectKey;
      skipAutoConnectRef.current = false;
      if (sessionActive && !busy) {
        void acpClose().then(() =>
          queryClient.invalidateQueries({ queryKey: ["acp-status"] }),
        );
      }
    }
  }, [connectKey, sessionActive, busy, queryClient]);

  useEffect(() => {
    if (statusQuery.isLoading) return;

    if (!available) {
      setConnectionState("unavailable");
      return;
    }

    if (busy) return;

    if (sessionActive) {
      setConnectionState("connected");
      return;
    }

    if (skipAutoConnectRef.current) {
      setConnectionState("idle");
      return;
    }

    let cancelled = false;
    setConnectionState("connecting");
    setProgress("正在连接 Agent…");

    const profileState = useAcpProfilesStore.getState();
    const settings = useAcpSettingsStore.getState();
    const session = useAcpSessionStore.getState();

    void acpConnect(
      (event: AcpEvent) => {
        if (cancelled) return;
        if (event.type === "progress") {
          setProgress(event.message);
        }
        if (event.type === "sessionSaved") {
          setSavedSession({
            sessionId: event.sessionId,
            profileId: event.profileId,
            cwd: event.cwd,
          });
        }
      },
      {
        profileId: profileState.activeProfileId,
        cwd: sessionCwd,
        savedSession: session.savedSession,
        clientSettings: {
          permissionMode: settings.permissionMode,
          thinkingLevel: settings.thinkingLevel,
          agentMode: settings.agentMode,
          visionCapable: settings.visionCapable ?? false,
        },
        profiles: profilesHintFromStore(
          profileState.activeProfileId,
          profileState.profiles,
        ),
      },
    )
      .then(async () => {
        if (cancelled) return;
        setConnectionState("connected");
        setProgress(null);
        await queryClient.invalidateQueries({ queryKey: ["acp-status"] });
      })
      .catch((error) => {
        if (cancelled) return;
        setConnectionState("error");
        setProgress(null);
        pushSystem(errorMessage(error));
      });

    return () => {
      cancelled = true;
    };
  }, [
    available,
    busy,
    connectAttempt,
    connectKey,
    queryClient,
    sessionActive,
    sessionCwd,
    setSavedSession,
    statusQuery.isLoading,
  ]);

  const handleReconnect = () => {
    skipAutoConnectRef.current = false;
    setConnectAttempt((attempt) => attempt + 1);
  };

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

      const profileState = useAcpProfilesStore.getState();
      const settings = useAcpSettingsStore.getState();
      const session = useAcpSessionStore.getState();

      try {
        return await acpPrompt(
          text,
          (event: AcpEvent) => handleEvent(event, turn.id, thinkingLevel),
          {
            profileId: profileState.activeProfileId,
            cwd: sessionCwd,
            context: videoContext,
            savedSession: session.savedSession,
            clientSettings: {
              permissionMode: settings.permissionMode,
              thinkingLevel: settings.thinkingLevel,
              agentMode: settings.agentMode,
              visionCapable: settings.visionCapable ?? false,
            },
            profiles: profilesHintFromStore(
              profileState.activeProfileId,
              profileState.profiles,
            ),
          },
        );
      } catch (error) {
        const message = errorMessage(error);
        const code =
          typeof error === "object" && error && "code" in error
            ? String((error as { code: string }).code)
            : "Error";
        setTurns((prev) =>
          prev.map((t) =>
            t.id === turn.id
              ? applyAcpEventToTurn(
                  t,
                  { type: "failed", code, message },
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
    if (!text || busy || !available || connectionState !== "connected") return;
    setDraft("");
    runMutation.mutate(text);
  };

  const startNewChat = () => {
    if (busy) return;
    if (isBlankChat && !sessionActive) return;
    setTurns([]);
    setNotices([]);
    setDraft("");
    setProgress(null);
    setPendingPermission(null);
    if (sessionActive) {
      closeMutation.mutate();
    } else {
      clearSavedSession();
    }
  };

  const scrollToTurn = (turnId: string) => {
    turnListRef.current
      ?.querySelector(`[data-turn-id="${turnId}"]`)
      ?.scrollIntoView({ behavior: "smooth", block: "nearest" });
  };

  const statusLine = statusQuery.isLoading
    ? null
    : busy
      ? "回合进行中…"
      : connectionState === "connected" && sessionCwd
        ? `工作目录：${sessionCwd}`
        : connectionState === "connecting"
          ? null
          : hasSavedSession
            ? "已记住会话 ID，下次连接将尝试 resume"
            : (statusQuery.data?.message ?? null);

  return (
    <ChatShell data-chat-shell={listKey}>
      <ChatToolbar
        agentLabel={agentLabel}
        connectionState={connectionState}
        statusLine={statusLine}
        statusError={
          statusQuery.isError ? errorMessage(statusQuery.error) : null
        }
        loading={statusQuery.isLoading}
        sessionActive={sessionActive}
        busy={busy}
        historyItems={historyItems}
        onNewChat={startNewChat}
        onPickHistory={scrollToTurn}
        onEndSession={() => closeMutation.mutate()}
        onReconnect={handleReconnect}
      />

      <div
        ref={turnListRef}
        className="chat-scroll min-h-0 flex-1 overflow-y-auto overscroll-y-contain"
      >
        <ChatTurnList turns={turns} notices={notices} />
      </div>

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
        disabled={!available || connectionState !== "connected"}
        busy={busy}
        placeholder={
          !available
            ? "请展开下方 Agent 设置并配置可用的 Agent"
            : connectionState === "connecting"
              ? "正在连接 Agent…"
              : connectionState === "error"
                ? "连接失败，请点击上方「重连」"
                : connectionState === "idle"
                  ? "Agent 未连接，请稍候或点击「重连」"
                  : "输入问题（Enter 发送，Shift+Enter 换行）"
        }
        onChange={setDraft}
        onSend={send}
        onCancel={() => {
          void acpCancel();
        }}
      />

      <AgentSettingsPanel status={statusQuery.data} busy={busy} />
    </ChatShell>
  );
}

