import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useId, useMemo, useRef, useState } from "react";

import { loadLatestAnnotationProposal, dismissAnnotationProposal } from "@/features/notes/api";
import {
  shouldFetchAnnotationProposal,
  shouldRescanTurnForProposal,
  shouldTrackProposeToolCall,
  turnNeedsProposalFetch,
} from "@/features/notes/annotationProposal";
import {
  applyAttachAnnotationProposal,
  applyClearTurnAnnotation,
  applySaveAnnotation,
  attachProposalBinding,
  canAttachProposalToTurn,
  createProposalBindings,
  seedProposalBindingsFromTurns,
  type ProposalBindings,
} from "@/features/notes/annotationTurnState";
import { usePlayerStore } from "@/features/player";
import { errorMessage } from "@/lib/format";

import "../chat-motion.css";

import { useAcpProfilesStore } from "@lumina/chat-ui/acpProfilesStore";
import { useAskAboutStore } from "@lumina/chat-ui/askAboutStore";
import {
  clientSettingsFromStore,
  useAcpSettingsStore,
} from "@lumina/chat-ui/acpSettingsStore";
import { useAcpSessionStore } from "@lumina/chat-ui/acpSessionStore";
import {
  acpCancel,
  acpClose,
  acpConnect,
  listAcpAgentSessions,
  acpNewChat,
  acpPrompt,
  getAcpStatus,
} from "../api";
import { profilesHintFromStore } from "@lumina/chat-ui/defaultAgentProfiles";
import {
  applyAcpEventToTurn,
  createTurn,
  pushNotice,
  syncTurnIdSeq,
  type SystemNotice,
} from "@lumina/chat-ui/chatTurns";
import {
  listConversationsForScope,
  useChatHistoryStore,
} from "@lumina/chat-ui/chatHistoryStore";
import {
  canAdoptResumeTarget,
  canUseVerifiedAgentSessions,
  formatConversationHistoryContext,
  reconcileConversations,
  resumeHintForConversation,
  shouldRequestHistorySessionList,
  shouldInjectHistoryContext,
} from "@lumina/chat-ui/conversationContext";
import { useChatUiStore } from "@lumina/chat-ui/chatUiStore";
import { buildAnchoredVideoPromptContext } from "../context";
import { workspaceCwdFromMedia } from "@lumina/player-ui/cwd";
import { profilesSignature } from "@lumina/chat-ui/profilesSignature";
import {
  bargeInPrompt,
  createQueuedPrompt,
  dequeuePrompt,
  enqueuePrompt,
  removeQueuedPrompt,
  type QueuedPrompt,
} from "@lumina/chat-ui/promptQueue";
import type {
  AcpConnectionState,
  AcpEvent,
  AgentSessionListResult,
  ChatTurn,
  PendingPermission,
  ThinkingLevel,
} from "../types";
import { useVideoPromptContext } from "../useVideoPromptContext";
import { useTypingPlaybackAnchor } from "../typingPlaybackAnchor";
import { AgentSettingsPanel } from "./AgentSettingsPanel";
import { ChatHistorySheet } from "@lumina/chat-ui/components/ChatHistorySheet";
import { ChatComposerBar, type ChatComposerBarHandle } from "./ChatComposerBar";
import { ChatShell } from "@lumina/chat-ui/components/ChatShell";
import { ChatColumn } from "@lumina/chat-ui/components/ChatShell";
import { ChatToolbar } from "./ChatToolbar";
import { ChatTurnList } from "./ChatTurnList";
import { PermissionPrompt } from "./PermissionPrompt";

type HistorySessionListSnapshot = {
  result: AgentSessionListResult;
  profileId: string;
  cwd: string | null;
};

/** Kept mounted in ChatDock after first open; hide ≠ unmount. */
export function AcpPanel() {
  const queryClient = useQueryClient();
  const currentFile = usePlayerStore((s) => s.currentFile);
  const thinkingLevel = useAcpSettingsStore((s) => s.thinkingLevel);
  const activeProfileId = useAcpProfilesStore((s) => s.activeProfileId);
  const profilesSig = useAcpProfilesStore((s) => profilesSignature(s.profiles));
  const savedSession = useAcpSessionStore((s) => s.savedSession);
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
  const busyRef = useRef(false);
  const drainLockRef = useRef(false);
  const [promptQueue, setPromptQueue] = useState<QueuedPrompt[]>([]);
  const promptQueueRef = useRef<QueuedPrompt[]>([]);
  const [progress, setProgress] = useState<string | null>(null);
  const [connectionState, setConnectionState] =
    useState<AcpConnectionState>("idle");
  const [connectAttempt, setConnectAttempt] = useState(0);
  const prevConnectKeyRef = useRef<string | null>(null);
  const [pendingPermission, setPendingPermission] =
    useState<PendingPermission | null>(null);
  const handledProposalIdsRef = useRef<Set<string>>(new Set());
  const proposalTurnByIdRef = useRef<Map<string, string>>(new Map());
  const proposeToolCallIdsRef = useRef<Set<string>>(new Set());
  const proposalBindingsRef = useRef<ProposalBindings>(createProposalBindings());

  const syncProposalBindings = () => {
    proposalBindingsRef.current = {
      handledProposalIds: handledProposalIdsRef.current,
      proposalTurnById: proposalTurnByIdRef.current,
    };
  };
  const [quickNoteOpen, setQuickNoteOpen] = useState(false);
  const [conversationId, setConversationId] = useState(
    () => `chat-${Date.now()}`,
  );
  const [historyOpen, setHistoryOpen] = useState(false);
  const [historyIncludeAll, setHistoryIncludeAll] = useState(false);
  const [agentSessionList, setAgentSessionList] =
    useState<HistorySessionListSnapshot | null>(null);
  const [historyListRequested, setHistoryListRequested] = useState(false);
  const historyListRequestSeqRef = useRef(0);
  const [historyInjectionActive, setHistoryInjectionActive] = useState(false);
  // 已注入过历史摘要的 Agent session；undefined = 尚未注入。
  const injectedHistorySessionRef = useRef<string | null | undefined>(
    undefined,
  );
  const resumeExpectedSessionIdRef = useRef<string | null>(null);
  const resumeNoticePendingRef = useRef(false);

  const available = statusQuery.data?.available ?? false;
  const sessionActive = statusQuery.data?.sessionActive ?? false;
  const sessionCwd = workspaceCwdFromMedia(currentFile);
  const connectKey = `${activeProfileId}:${profilesSig}:${sessionCwd ?? ""}`;
  const videoContext = useVideoPromptContext();
  const {
    handleDraftChange,
    clearTypingAnchor,
    seedAnchorPositionMs,
    consumeAnchorPositionMs,
  } = useTypingPlaybackAnchor();
  const setDraftEmpty = () => {
    clearTypingAnchor();
    setDraft("");
  };

  const onDraftChange = (next: string) => {
    handleDraftChange(next);
    setDraft(next);
  };

  // P6-M3 shortcut-ask: seed the anchor at the asked-about time BEFORE the
  // draft change, so the idle window keeps it instead of re-anchoring live.
  const askAboutRequest = useAskAboutStore((s) => s.request);
  useEffect(() => {
    if (!askAboutRequest) return;
    seedAnchorPositionMs(askAboutRequest.anchorMs);
    handleDraftChange(askAboutRequest.text);
    setDraft(askAboutRequest.text);
    useAskAboutStore.getState().consume();
    useChatUiStore.getState().openChat();
    window.setTimeout(() => composerRef.current?.focusInput(), 0);
    window.setTimeout(() => composerRef.current?.focusInput(), 120);
  }, [askAboutRequest, seedAnchorPositionMs, handleDraftChange]);

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

  const handleSessionSaved = (
    event: Extract<AcpEvent, { type: "sessionSaved" }>,
  ) => {
    const expectedSessionId = resumeExpectedSessionIdRef.current;
    if (expectedSessionId) {
      if (event.sessionId === expectedSessionId) {
        // The requested Agent history is still attached to this session, so
        // the armed fallback summary remains suppressed by this id match.
        injectedHistorySessionRef.current = event.sessionId;
        if (resumeNoticePendingRef.current) {
          pushSystem("已恢复该对话的 AI 记忆");
        }
      } else {
        // Keep the requested id in the ledger. The new id will therefore
        // cause shouldInjectHistoryContext to arm the fallback on the next
        // prompt, including when resume failed inside acp_prompt.
        setHistoryInjectionActive(true);
        if (resumeNoticePendingRef.current) {
          pushSystem("该对话的 AI 记忆暂不可用，已加载本地历史记录");
        }
      }
      resumeExpectedSessionIdRef.current = null;
      resumeNoticePendingRef.current = false;
    }
    setSavedSession({
      sessionId: event.sessionId,
      profileId: event.profileId,
      cwd: event.cwd,
    });
  };

  const clearHistorySessionListState = () => {
    historyListRequestSeqRef.current += 1;
    setHistoryListRequested(false);
    setAgentSessionList(null);
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
      agentSessionId:
        savedSession?.profileId === activeProfileId &&
        savedSession.cwd === (sessionCwd ?? null)
          ? savedSession.sessionId
          : null,
      turns,
    });
  }, [
    activeProfileId,
    busy,
    conversationId,
    sessionCwd,
    savedSession,
    turns,
    upsertActiveConversation,
  ]);

  const newChatMutation = useMutation({
    mutationFn: async (options?: { preserveTurns?: boolean }) => {
      const preserveTurns = options?.preserveTurns ?? false;
      resumeExpectedSessionIdRef.current = null;
      resumeNoticePendingRef.current = false;
      clearSavedSession();
      if (!preserveTurns) {
        setTurns([]);
        setNotices([]);
        setDraftEmpty();
        setHistoryInjectionActive(false);
        injectedHistorySessionRef.current = undefined;
        promptQueueRef.current = [];
        setPromptQueue([]);
        handledProposalIdsRef.current.clear();
        proposalTurnByIdRef.current.clear();
        proposeToolCallIdsRef.current.clear();
        proposalBindingsRef.current = createProposalBindings();
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
            handleSessionSaved(event);
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
        clearHistorySessionListState();
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

  const reconciledHistory = useMemo(
    () => {
      const listScopeMatches =
        agentSessionList?.profileId === activeProfileId &&
        agentSessionList.cwd === (sessionCwd ?? null);
      const result = listScopeMatches ? agentSessionList.result : null;
      return reconcileConversations(scopedHistory, result?.sessions, {
        verified: canUseVerifiedAgentSessions({
          hasData: result !== null,
          verified: result?.verified ?? false,
          truncated: result?.truncated ?? false,
          busy: composerBusy,
        }),
        queryScope: {
          profileId: activeProfileId,
          cwd: sessionCwd ?? null,
        },
      });
    },
    [activeProfileId, agentSessionList, composerBusy, scopedHistory, sessionCwd],
  );

  const handleHistoryOpenChange = (open: boolean) => {
    if (!open) {
      clearHistorySessionListState();
      setHistoryOpen(false);
      return;
    }

    setHistoryOpen(true);
    if (
      !shouldRequestHistorySessionList({
        historyOpen: true,
        connected: connectionState === "connected",
        busy: composerBusy,
        requested: historyListRequested,
      })
    ) {
      return;
    }

    const requestSeq = historyListRequestSeqRef.current + 1;
    historyListRequestSeqRef.current = requestSeq;
    setHistoryListRequested(true);
    setAgentSessionList(null);
    void listAcpAgentSessions(sessionCwd)
      .then((result) => {
        if (historyListRequestSeqRef.current !== requestSeq) return;
        setAgentSessionList({
          result,
          profileId: activeProfileId,
          cwd: sessionCwd ?? null,
        });
      })
      .catch(() => {
        if (historyListRequestSeqRef.current !== requestSeq) return;
        setAgentSessionList({
          result: {
            verified: false,
            sessions: [],
            truncated: false,
          },
          profileId: activeProfileId,
          cwd: sessionCwd ?? null,
        });
        pushSystem("暂时无法校验历史对话，仍可使用本地记录");
      });
  };

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
          handleSessionSaved(event);
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

  const seedHandledProposals = (nextTurns: ChatTurn[]) => {
    const bindings = seedProposalBindingsFromTurns(nextTurns);
    handledProposalIdsRef.current = bindings.handledProposalIds;
    proposalTurnByIdRef.current = bindings.proposalTurnById;
    proposalBindingsRef.current = bindings;
  };

  const tryLoadAnnotationProposal = async (turnId: string) => {
    if (!sessionCwd) return;
    try {
      const proposal = await loadLatestAnnotationProposal(sessionCwd);
      if (!proposal) return;
      syncProposalBindings();
      if (
        !canAttachProposalToTurn(
          proposal.proposalId,
          turnId,
          proposalBindingsRef.current,
        )
      ) {
        return;
      }
      attachProposalBinding(
        proposal.proposalId,
        turnId,
        proposalBindingsRef.current,
      );
      handledProposalIdsRef.current =
        proposalBindingsRef.current.handledProposalIds;
      proposalTurnByIdRef.current =
        proposalBindingsRef.current.proposalTurnById;
      setTurns((prev) =>
        applyAttachAnnotationProposal(prev, turnId, proposal),
      );
    } catch (error) {
      pushSystem(errorMessage(error));
    }
  };

  const clearTurnAnnotation = (turnId: string) => {
    setTurns((prev) => applyClearTurnAnnotation(prev, turnId));
  };

  const handleDismissAnnotation = (turnId: string) => {
    if (sessionCwd) {
      void dismissAnnotationProposal(sessionCwd).catch((error) => {
        pushSystem(errorMessage(error));
      });
    }
    clearTurnAnnotation(turnId);
  };

  const handleSaveAnnotation = (turnId: string, proposalId?: string) => {
    setTurns((prev) => applySaveAnnotation(prev, turnId, proposalId));
    if (proposalId) {
      syncProposalBindings();
      attachProposalBinding(
        proposalId,
        turnId,
        proposalBindingsRef.current,
      );
      handledProposalIdsRef.current =
        proposalBindingsRef.current.handledProposalIds;
      proposalTurnByIdRef.current =
        proposalBindingsRef.current.proposalTurnById;
    }
    pushSystem("批注已写入笔记库");
  };

  useEffect(() => {
    if (!sessionCwd) return;
    const pendingTurn = [...turns]
      .reverse()
      .find((turn) => shouldRescanTurnForProposal(turn, turns));
    if (!pendingTurn) return;
    void tryLoadAnnotationProposal(pendingTurn.id);
  }, [sessionCwd, turns]);

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
        handleSessionSaved(event);
        break;
      case "finished":
      case "failed":
      case "agentMessage":
      case "agentThought":
      case "toolCall":
      case "toolCallUpdate":
      case "plan": {
        if (event.type === "toolCall" && shouldTrackProposeToolCall(event)) {
          proposeToolCallIdsRef.current.add(event.toolCallId);
        }
        let fetchProposal = false;
        setTurns((prev) => {
          const current = prev.find((turn) => turn.id === turnId);
          const activities = current?.activities;
          if (
            event.type === "toolCallUpdate" &&
            shouldFetchAnnotationProposal(event, {
              trackedProposeToolCallIds: proposeToolCallIdsRef.current,
              activities,
            })
          ) {
            proposeToolCallIdsRef.current.delete(event.toolCallId);
            fetchProposal = true;
          }
          const next = prev.map((turn) =>
            turn.id === turnId ? applyAcpEventToTurn(turn, event, level) : turn,
          );
          if (event.type === "finished") {
            const updated = next.find((turn) => turn.id === turnId);
            if (updated && turnNeedsProposalFetch(updated)) {
              fetchProposal = true;
            }
          }
          return next;
        });
        if (fetchProposal) {
          void tryLoadAnnotationProposal(turnId);
        }
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
    }
  };

  const syncPromptQueue = (next: QueuedPrompt[]) => {
    promptQueueRef.current = next;
    setPromptQueue(next);
  };

  const runMutation = useMutation({
    mutationFn: async ({
      text,
      anchorPositionMs,
    }: {
      text: string;
      anchorPositionMs: number;
    }) => {
      busyRef.current = true;
      setBusy(true);
      setProgress(null);
      setPendingPermission(null);

      const turn = createTurn(idSeq, text, anchorPositionMs);
      setTurns((prev) => [...prev, turn]);

      const profileState = useAcpProfilesStore.getState();
      const settings = clientSettingsFromStore(useAcpSettingsStore.getState());
      const session = useAcpSessionStore.getState();
      // 恢复历史对话只注入一次：注入成功后 Agent 会话自己保持上下文，
      // 每轮重发整段摘要纯属浪费 token 与首字延迟。
      const historyContext = shouldInjectHistoryContext(
        {
          armed: historyInjectionActive,
          injectedSessionId: injectedHistorySessionRef.current,
        },
        session.savedSession?.sessionId ?? null,
      )
        ? formatConversationHistoryContext(turns)
        : null;
      if (historyInjectionActive && session.savedSession?.sessionId) {
        resumeExpectedSessionIdRef.current = session.savedSession.sessionId;
      }

      try {
        const player = usePlayerStore.getState();
        const frozenContext = buildAnchoredVideoPromptContext({
          base: videoContext,
          anchorPositionMs,
          durationMs: player.durationMs,
        });
        const result = await acpPrompt(
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
        if (historyContext) {
          injectedHistorySessionRef.current =
            useAcpSessionStore.getState().savedSession?.sessionId ?? null;
        }
        return result;
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
      busyRef.current = false;
      drainLockRef.current = false;
      setBusy(false);
      setProgress(null);
      void queryClient.invalidateQueries({ queryKey: ["acp-status"] });
      window.setTimeout(() => composerRef.current?.focusInput(), 0);
      window.setTimeout(() => composerRef.current?.focusInput(), 120);
    },
  });

  const launchNextQueuedPrompt = () => {
    if (
      drainLockRef.current ||
      busyRef.current ||
      runMutation.isPending ||
      newChatMutation.isPending
    ) {
      return;
    }
    if (!available || connectionState !== "connected") return;
    const { next, rest } = dequeuePrompt(promptQueueRef.current);
    if (!next) return;
    drainLockRef.current = true;
    busyRef.current = true;
    syncPromptQueue(rest);
    runMutation.mutate({
      text: next.text,
      anchorPositionMs: next.anchorPositionMs,
    });
  };

  useEffect(() => {
    if (busy || composerBusy) return;
    if (!available || connectionState !== "connected") return;
    if (promptQueue.length === 0) return;
    launchNextQueuedPrompt();
  }, [
    available,
    busy,
    composerBusy,
    connectionState,
    promptQueue,
  ]);

  const send = () => {
    const text = draft.trim();
    if (!text || !available || connectionState !== "connected") return;
    const anchorPositionMs = consumeAnchorPositionMs();
    if (busy || busyRef.current || runMutation.isPending) {
      syncPromptQueue(
        enqueuePrompt(
          promptQueueRef.current,
          createQueuedPrompt(text, anchorPositionMs),
        ),
      );
      setDraft("");
      return;
    }
    setDraft("");
    runMutation.mutate({ text, anchorPositionMs });
  };

  const bargeIn = () => {
    const text = draft.trim();
    if (!text || !available || connectionState !== "connected") return;
    if (!(busy || busyRef.current || runMutation.isPending)) {
      send();
      return;
    }
    const anchorPositionMs = consumeAnchorPositionMs();
    syncPromptQueue(
      bargeInPrompt(
        promptQueueRef.current,
        createQueuedPrompt(text, anchorPositionMs),
      ),
    );
    setDraft("");
    void acpCancel();
  };

  const cancelCurrentTurn = () => {
    const queued = promptQueueRef.current.length;
    // 取消要经过后端（等在跑的工具返回＋最多 8 秒优雅期），先给即时反馈，
    // 否则这段时间 UI 没有任何变化，看起来像"取消没传达过去"。
    setProgress("正在取消…");
    void acpCancel();
    if (queued > 0) {
      pushSystem(`已取消当前回合，将继续发送排队中的 ${queued} 条`);
    }
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
      syncPromptQueue([]);
      clearSavedSession();
      return;
    }
    syncPromptQueue([]);
    newChatMutation.mutate();
  };

  const loadConversation = (id: string) => {
    const item = conversations.find((conversation) => conversation.id === id);
    if (!item) return;
    const resumeHint = resumeHintForConversation(item, {
      profileId: activeProfileId,
      cwd: sessionCwd ?? null,
    });
    const currentSessionId = useAcpSessionStore.getState().savedSession
      ?.sessionId;
    const activeTargetMatches =
      resumeHint !== null &&
      sessionActive &&
      currentSessionId === resumeHint.sessionId;
    // 正忙时无法接管目标会话，此时必须退回摘要兜底（见 canAdoptResumeTarget）。
    const adoptResumeTarget =
      resumeHint !== null &&
      canAdoptResumeTarget({
        targetIsLiveSession: activeTargetMatches,
        sessionActive,
        transitionBlocked: busy || newChatMutation.isPending,
      });
    setConversationId(item.id);
    setActiveConversationId(item.id);
    syncTurnIdSeq(idSeq, item.turns);
    seedHandledProposals(item.turns);
    syncPromptQueue([]);
    setTurns(item.turns);
    setNotices([]);
    setDraftEmpty();
    setProgress(null);
    setPendingPermission(null);
    handleHistoryOpenChange(false);
    setHistoryInjectionActive(true);

    if (adoptResumeTarget && resumeHint) {
      injectedHistorySessionRef.current = resumeHint.sessionId;
      resumeExpectedSessionIdRef.current = resumeHint.sessionId;
      resumeNoticePendingRef.current = true;
      setSavedSession(resumeHint);
      if (activeTargetMatches) {
        resumeExpectedSessionIdRef.current = null;
        resumeNoticePendingRef.current = false;
        pushSystem("已恢复该对话的 AI 记忆");
      } else {
        pushSystem("正在恢复该对话的 AI 记忆…");
      }
    } else {
      injectedHistorySessionRef.current = undefined;
      resumeExpectedSessionIdRef.current = null;
      resumeNoticePendingRef.current = false;
      // 无 hint 时清掉会话指针；有 hint 但此刻接管不了则保持现状，
      // 让当前会话继续服务，靠一次性摘要补上下文。
      if (!resumeHint) clearSavedSession();
      pushSystem("已恢复历史对话，继续提问将带上此前上下文");
    }

    if (adoptResumeTarget && !activeTargetMatches) {
      if (sessionActive) {
        setConnectionState("connecting");
        setProgress("正在恢复该对话的 AI 记忆…");
        void acpClose()
          .then(() =>
            queryClient.invalidateQueries({ queryKey: ["acp-status"] }),
          )
          .catch((error) => {
            setConnectionState("error");
            setProgress(null);
            pushSystem(errorMessage(error));
          });
      }
      return;
    }

    if (
      !adoptResumeTarget &&
      sessionActive &&
      !busy &&
      !newChatMutation.isPending
    ) {
      newChatMutation.mutate({ preserveTurns: true });
    }
  };

  const statusLine = statusQuery.isLoading
    ? null
    : busy
      ? promptQueue.length > 0
        ? `回合进行中…（已排队 ${promptQueue.length} 条）`
        : "回合进行中…"
      : promptQueue.length > 0
        ? `排队 ${promptQueue.length} 条，即将发送…`
        : connectionState === "connected" && sessionCwd
        ? `工作目录：${sessionCwd}`
        : connectionState === "connecting"
          ? null
          : hasSavedSession
            ? "已记住该对话的 AI 记忆，下次连接将自动尝试恢复"
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
          onOpenHistory={() => {
            setQuickNoteOpen(false);
            handleHistoryOpenChange(!historyOpen);
          }}
          quickNoteDisabled={!currentFile}
          quickNoteOpen={quickNoteOpen}
          onQuickNoteOpenChange={(open) => {
            if (open) handleHistoryOpenChange(false);
            setQuickNoteOpen(open);
          }}
          onQuickNoteSaved={() => pushSystem("批注已保存")}
          onReconnect={handleReconnect}
        />
        <ChatHistorySheet
          open={historyOpen}
          items={reconciledHistory}
          activeId={activeConversationId ?? conversationId}
          scopeLabel="当前视频"
          includeAll={historyIncludeAll}
          onToggleScope={() => setHistoryIncludeAll((value) => !value)}
          onClose={() => handleHistoryOpenChange(false)}
          onSelect={loadConversation}
          onDelete={deleteConversation}
        />
      </ChatColumn>

      <div
        ref={turnListRef}
        className="chat-scroll min-h-0 flex-1 overflow-y-auto overscroll-y-contain"
      >
        <ChatTurnList
          turns={turns}
          notices={notices}
          annotationWorkspace={sessionCwd}
          onDismissAnnotation={handleDismissAnnotation}
          onSaveAnnotation={handleSaveAnnotation}
        />
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
        queue={promptQueue}
        onSend={send}
        onBargeIn={bargeIn}
        onCancel={cancelCurrentTurn}
        onRemoveQueued={(id) => {
          syncPromptQueue(removeQueuedPrompt(promptQueueRef.current, id));
        }}
        onClearQueue={() => syncPromptQueue([])}
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

