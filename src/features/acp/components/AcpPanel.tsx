import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useId, useMemo, useRef, useState } from "react";

import { usePlayerStore } from "@/features/player";
import type { MediaInfo } from "@/features/media/types";
import type { Note } from "@/features/notes/types";
import { errorMessage } from "@/lib/format";

import "../chat-motion.css";

import { useAcpProfilesStore } from "../acpProfilesStore";
import {
  clientSettingsFromStore,
  useAcpSettingsStore,
} from "../acpSettingsStore";
import { useAcpSessionStore } from "../acpSessionStore";
import {
  acpCancel,
  acpClose,
  acpConnect,
  acpNewChat,
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
import {
  listConversationsForScope,
  useChatHistoryStore,
} from "../chatHistoryStore";
import { formatConversationHistoryContext } from "../conversationContext";
import { useChatUiStore } from "../chatUiStore";
import { buildAnchoredVideoPromptContext } from "../context";
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
import { useTypingPlaybackAnchor } from "../typingPlaybackAnchor";
import { AgentSettingsPanel } from "./AgentSettingsPanel";
import { ChatHistorySheet } from "./ChatHistorySheet";
import { ChatComposerBar, type ChatComposerBarHandle } from "./ChatComposerBar";
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
  const conversations = useChatHistoryStore((s) => s.conversations);
  const activeConversationId = useChatHistoryStore((s) => s.activeConversationId);
  const upsertActiveConversation = useChatHistoryStore(
    (s) => s.upsertActiveConversation,
  );
  const setActiveConversationId = useChatHistoryStore(
    (s) => s.setActiveConversationId,
  );
  const deleteConversation = useChatHistoryStore((s) => s.deleteConversation);
  const setAcpResponding = useChatUiStore((s) => s.setAcpResponding);

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
  const composerRef = useRef<ChatComposerBarHandle | null>(null);
  const [draft, setDraft] = useState("");
  const [turns, setTurns] = useState<ChatTurn[]>([]);
  const [notices, setNotices] = useState<SystemNotice[]>([]);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<string | null>(null);
  const [connectionState, setConnectionState] =
    useState<AcpConnectionState>("idle");
  const [connectAttempt, setConnectAttempt] = useState(0);
  const prevConnectKeyRef = useRef<string | null>(null);
  const [pendingPermission, setPendingPermission] =
    useState<PendingPermission | null>(null);
  const [conversationId, setConversationId] = useState(
    () => `chat-${Date.now()}`,
  );
  const [historyOpen, setHistoryOpen] = useState(false);
  const [historyIncludeAll, setHistoryIncludeAll] = useState(false);
  const [historyInjectionActive, setHistoryInjectionActive] = useState(false);

  const available = statusQuery.data?.available ?? false;
  const sessionActive = statusQuery.data?.sessionActive ?? false;
  const sessionCwd = workspaceCwdFromMedia(currentFile);
  const connectKey = `${activeProfileId}:${profilesSig}:${sessionCwd ?? ""}`;
  const videoContext = useVideoPromptContext();
  const { handleDraftChange, clearTypingAnchor, consumeAnchorPositionMs } =
    useTypingPlaybackAnchor();

  const setDraftEmpty = () => {
    clearTypingAnchor();
    setDraft("");
  };

  const onDraftChange = (next: string) => {
    handleDraftChange(next);
    setDraft(next);
  };

  const activeProfile = statusQuery.data?.profiles.find(
    (profile) => profile.id === activeProfileId,
  );
  const agentLabel = activeProfile?.name ?? "Agent";

  const chatTitle = useMemo(() => {
    const first = turns.find((turn) => turn.userText.trim());
    return first?.userText.trim().slice(0, 72) ?? null;
  }, [turns]);

  const scopedHistory = useMemo(
    () =>
      listConversationsForScope(
        conversations,
        sessionCwd,
        activeProfileId,
        historyIncludeAll,
      ),
    [conversations, sessionCwd, activeProfileId, historyIncludeAll],
  );

  const isBlankChat = turns.length === 0 && notices.length === 0;

  const pushSystem = (content: string) => {
    setNotices((prev) => pushNotice(prev, idSeq, content));
  };

  useEffect(() => {
    if (busy) return;
    if (turns.every((turn) => !turn.userText.trim() && !turn.answer.trim())) {
      return;
    }
    upsertActiveConversation({
      id: conversationId,
      cwd: sessionCwd ?? null,
      profileId: activeProfileId,
      turns,
    });
  }, [
    activeProfileId,
    busy,
    conversationId,
    sessionCwd,
    turns,
    upsertActiveConversation,
  ]);

  const newChatMutation = useMutation({
    mutationFn: async (options?: { preserveTurns?: boolean }) => {
      const preserveTurns = options?.preserveTurns ?? false;
      clearSavedSession();
      if (!preserveTurns) {
        setTurns([]);
        setNotices([]);
        setDraftEmpty();
        setHistoryInjectionActive(false);
      }
      setProgress(null);
      setPendingPermission(null);
      setConnectionState("connecting");
      setProgress(preserveTurns ? "正在同步 Agent 会话…" : "正在开始新对话…");

      const profileState = useAcpProfilesStore.getState();
      const settings = clientSettingsFromStore(useAcpSettingsStore.getState());

      await acpNewChat(
        (event: AcpEvent) => {
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
          clientSettings: settings,
          profiles: profilesHintFromStore(
            profileState.activeProfileId,
            profileState.profiles,
          ),
        },
      );
    },
    onSuccess: async (_data, options) => {
      setConnectionState("connected");
      setProgress(null);
      if (!options?.preserveTurns) {
        setConversationId(`chat-${Date.now()}`);
        setActiveConversationId(null);
        setHistoryOpen(false);
      }
      await queryClient.invalidateQueries({ queryKey: ["acp-status"] });
    },
    onError: (error) => {
      setConnectionState("error");
      setProgress(null);
      pushSystem(errorMessage(error));
    },
  });

  const composerBusy = busy || newChatMutation.isPending;

  useEffect(() => {
    setAcpResponding(composerBusy);
    return () => setAcpResponding(false);
  }, [composerBusy, setAcpResponding]);

  useEffect(() => {
    if (prevConnectKeyRef.current !== connectKey) {
      prevConnectKeyRef.current = connectKey;
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

    if (newChatMutation.isPending) return;

    if (sessionActive) {
      setConnectionState("connected");
      return;
    }

    let cancelled = false;
    setConnectionState("connecting");
    setProgress("正在连接 Agent…");

    const profileState = useAcpProfilesStore.getState();
    const settings = clientSettingsFromStore(useAcpSettingsStore.getState());
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
        clientSettings: settings,
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
    newChatMutation.isPending,
  ]);

  const handleReconnect = () => {
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
    mutationFn: async ({
      text,
      anchorPositionMs,
    }: {
      text: string;
      anchorPositionMs: number;
    }) => {
      setBusy(true);
      setProgress(null);
      setPendingPermission(null);

      const turn = createTurn(idSeq, text);
      const historyContext = historyInjectionActive
        ? formatConversationHistoryContext(turns)
        : null;
      setTurns((prev) => [...prev, turn]);

      const profileState = useAcpProfilesStore.getState();
      const settings = clientSettingsFromStore(useAcpSettingsStore.getState());
      const session = useAcpSessionStore.getState();

      try {
        const player = usePlayerStore.getState();
        const mediaPath = player.currentFile;
        const chapters = mediaPath
          ? queryClient.getQueryData<MediaInfo>(["mediaInfo", mediaPath])
              ?.chapters
          : undefined;
        const notes = mediaPath
          ? queryClient.getQueryData<Note[]>(["notes", mediaPath])
          : undefined;
        const frozenContext = buildAnchoredVideoPromptContext({
          base: videoContext,
          anchorPositionMs,
          durationMs: player.durationMs,
          chapters,
          notes,
        });
        return await acpPrompt(
          text,
          (event: AcpEvent) => handleEvent(event, turn.id, thinkingLevel),
          {
            profileId: profileState.activeProfileId,
            cwd: sessionCwd,
            context: frozenContext,
            historyContext,
            savedSession: session.savedSession,
            clientSettings: settings,
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
      window.setTimeout(() => composerRef.current?.focusInput(), 0);
      window.setTimeout(() => composerRef.current?.focusInput(), 120);
    },
  });

  const send = () => {
    const text = draft.trim();
    if (!text || busy || !available || connectionState !== "connected") return;
    const anchorPositionMs = consumeAnchorPositionMs();
    setDraft("");
    runMutation.mutate({ text, anchorPositionMs });
  };

  const startNewChat = () => {
    if (busy || newChatMutation.isPending) return;
    if (
      isBlankChat &&
      sessionActive &&
      connectionState === "connected"
    ) {
      return;
    }
    if (!available) {
      setTurns([]);
      setNotices([]);
      setDraftEmpty();
      setProgress(null);
      setPendingPermission(null);
      clearSavedSession();
      return;
    }
    newChatMutation.mutate();
  };

  const loadConversation = (id: string) => {
    const item = conversations.find((conversation) => conversation.id === id);
    if (!item) return;
    setConversationId(item.id);
    setActiveConversationId(item.id);
    setTurns(item.turns);
    setNotices([]);
    setDraftEmpty();
    setProgress(null);
    setPendingPermission(null);
    setHistoryOpen(false);
    setHistoryInjectionActive(true);
    pushSystem("已恢复历史对话，继续提问将带上此前上下文");

    if (
      sessionActive &&
      connectionState === "connected" &&
      !busy &&
      !newChatMutation.isPending
    ) {
      newChatMutation.mutate({ preserveTurns: true });
    }
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
      <ChatColumn className="sticky top-0 z-20 shrink-0 bg-card">
        <ChatToolbar
          agentLabel={agentLabel}
          chatTitle={chatTitle}
          connectionState={connectionState}
          statusLine={statusLine}
          statusError={
            statusQuery.isError ? errorMessage(statusQuery.error) : null
          }
          loading={statusQuery.isLoading}
          busy={composerBusy}
          historyCount={scopedHistory.length}
          onNewChat={startNewChat}
          onOpenHistory={() => setHistoryOpen((open) => !open)}
          onReconnect={handleReconnect}
        />
        <ChatHistorySheet
          open={historyOpen}
          items={scopedHistory}
          activeId={activeConversationId ?? conversationId}
          scopeLabel="当前视频"
          includeAll={historyIncludeAll}
          onToggleScope={() => setHistoryIncludeAll((value) => !value)}
          onClose={() => setHistoryOpen(false)}
          onSelect={loadConversation}
          onDelete={deleteConversation}
        />
      </ChatColumn>

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

      <ChatComposerBar
        ref={composerRef}
        value={draft}
        disabled={!available || connectionState !== "connected"}
        busy={composerBusy}
        status={statusQuery.data}
        sessionConnected={connectionState === "connected"}
        placeholder={
          !available
            ? "请展开下方 Agent 设置并配置可用的 Agent"
            : newChatMutation.isPending || connectionState === "connecting"
              ? "正在连接 Agent…"
              : connectionState === "error"
                ? "连接失败，请点击上方「重连」"
                : connectionState === "idle"
                  ? "Agent 未连接，请点击上方「重连」"
                  : "输入问题（Enter 发送，Shift+Enter 换行）"
        }
        onChange={onDraftChange}
        onSend={send}
        onCancel={() => {
          void acpCancel();
        }}
      />

      <AgentSettingsPanel
        status={statusQuery.data}
        busy={composerBusy}
        sessionConnected={connectionState === "connected"}
        sessionCwd={sessionCwd}
      />
    </ChatShell>
  );
}

