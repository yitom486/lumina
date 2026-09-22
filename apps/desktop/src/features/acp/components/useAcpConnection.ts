import {
  useEffect,
  useRef,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react";
import { useMutation, type QueryClient } from "@tanstack/react-query";

import { errorMessage } from "@/lib/format";
import { useAcpProfilesStore } from "@lumina/chat-ui/acpProfilesStore";
import {
  clientSettingsFromStore,
  useAcpSettingsStore,
} from "@lumina/chat-ui/acpSettingsStore";
import { useAcpSessionStore } from "@lumina/chat-ui/acpSessionStore";
import { syncTurnIdSeq, type SystemNotice } from "@lumina/chat-ui/chatTurns";
import { resumeOutcomeNotice } from "@lumina/chat-ui/conversationContext";
import { profilesHintFromStore } from "@lumina/chat-ui/defaultAgentProfiles";

import { acpClose, acpConnect, acpNewChat, acpSwitchSession } from "../api";
import type {
  AcpConnectionState,
  AcpEvent,
  ChatTurn,
  PendingPermission,
  ResumeOutcome,
  SavedSessionHint,
} from "../types";
import { readChatRestore, type ChatRestoreSnapshot } from "../chatRestore";
import { mapLoadedTranscript } from "../conversationTranscript";
import { claimProgressOwner, type ProgressOwner } from "../progressOwner";
import { fetchAgentTranscript } from "../queries";

/** 原 AcpPanel 内的首句标题 helper：本轮见过的用户首句覆盖原生垃圾标题。 */
export function firstUserTextOf(turns: ChatTurn[]): string | null {
  const text = turns
    .find((turn) => turn.userText.trim())
    ?.userText.trim()
    .slice(0, 64);
  return text || null;
}

export type UseAcpConnectionInput = {
  queryClient: QueryClient;
  sessionCwd: string | undefined;
  profilesSig: string;
  available: boolean;
  sessionActive: boolean;
  statusLoading: boolean;
  savedSession: SavedSessionHint | null;
  promptBusy: boolean;
  turnsRef: { current: ChatTurn[] };
  idSeq: { n: number };
  pushSystem: (content: string) => void;
  setTurns: Dispatch<SetStateAction<ChatTurn[]>>;
  setNotices: Dispatch<SetStateAction<SystemNotice[]>>;
  clearDraft: () => void;
  clearPromptQueue: () => void;
  clearProposals: () => void;
  setPendingPermission: (value: PendingPermission | null) => void;
  seedHandledProposals: (turns: ChatTurn[]) => void;
  initialRestore: ChatRestoreSnapshot | null;
};

/**
 * 连接域（零行为搬运，原 AcpPanel 内逻辑）：连接态/对账 ref/切换 epoch、
 * handleSessionSaved、新建/切换会话 mutation、connectKey + 两 connect effect。
 */
export function useAcpConnection(input: UseAcpConnectionInput) {
  const {
    queryClient,
    sessionCwd,
    profilesSig,
    available,
    sessionActive,
    statusLoading,
    savedSession,
    promptBusy,
    turnsRef,
    idSeq,
    pushSystem,
    setTurns,
    setNotices,
    clearDraft,
    clearPromptQueue,
    clearProposals,
    setPendingPermission,
    seedHandledProposals,
    initialRestore,
  } = input;

  const activeProfileId = useAcpProfilesStore((s) => s.activeProfileId);
  const setSavedSessionFor = useAcpSessionStore((s) => s.setSavedSessionFor);
  const clearSavedSessionFor = useAcpSessionStore(
    (s) => s.clearSavedSessionFor,
  );

  const [connectionState, setConnectionState] =
    useState<AcpConnectionState>("idle");
  const [connectAttempt, setConnectAttempt] = useState(0);
  const prevConnectKeyRef = useRef<string | null>(null);
  const [progress, setProgress] = useState<string | null>(null);
  const [lastSessionResolution, setLastSessionResolution] = useState<{
    outcome: ResumeOutcome | "fresh";
    sessionId: string;
  } | null>(null);
  const [sessionBanner, setSessionBanner] = useState<string | null>(null);
  const [titleOverrides, setTitleOverrides] = useState<Record<string, string>>(
    {},
  );
  const resumeExpectedSessionIdRef = useRef<string | null>(null);
  const resumeNoticePendingRef = useRef(false);
  const progressOwnerRef = useRef<ProgressOwner>(null);
  const transcriptLoadSeqRef = useRef(0);
  // 秒开恢复的对账基线：尝试恢复的正是摆出来的那个旧会话、且用户还没
  // 说过话，后端却落了另一个新会话 → 清掉缓存，不把旧线程盖在新会话上。
  const restoreRef = useRef<{
    sessionId: string | null;
    turnCount: number;
  } | null>(
    initialRestore
      ? {
          sessionId:
            useAcpSessionStore.getState().savedSessionFor(initialRestore.profileId)
              ?.sessionId ?? null,
          turnCount: initialRestore.turns.length,
        }
      : null,
  );
  const resumeAttemptedRef = useRef<SavedSessionHint | null>(null);
  // 画像切换 epoch：运行中切换画像时记下目标画像，connect effect 认领后
  // 必走“先关旧、再连新”，不再信任 sessionActive 快捷返回。
  const switchEpochRef = useRef<{ profileId: string } | null>(null);

  const connectKey = `${activeProfileId}:${profilesSig}:${sessionCwd ?? ""}`;

  // 后端每次落定会话都会发 sessionSaved：无条件记录，供工具栏常驻显示。
  const handleSessionSaved = (
    event: Extract<AcpEvent, { type: "sessionSaved" }>,
  ) => {
    const expectedSessionId = resumeExpectedSessionIdRef.current;
    const shortId = event.sessionId.slice(0, 8);
    if (expectedSessionId) {
      if (resumeNoticePendingRef.current) {
        setSessionBanner(
          `${resumeOutcomeNotice({
            outcome: event.resume,
            sessionMatchedRequest: event.sessionId === expectedSessionId,
          })}（${shortId}）`,
        );
      }
      resumeExpectedSessionIdRef.current = null;
      resumeNoticePendingRef.current = false;
    } else if (event.resume) {
      // 自动路径（重连/报错重建）以前静默：恢复成功用户不知道，
      // 旧记忆丢失也只剩一个常驻顶栏。现在落定即换顶部横条。
      setSessionBanner(
        `${resumeOutcomeNotice({
          outcome: event.resume,
          sessionMatchedRequest: false,
        })}（${shortId}）`,
      );
    } else {
      setSessionBanner(`已连接，开始新对话（${shortId}）`);
    }
    setLastSessionResolution({
      outcome: event.resume ?? "fresh",
      sessionId: event.sessionId,
    });
    // resume 成功但面板是空的（目标画像本地快照缺失）：把远端真实文本捞回来。
    // 单 stdio 上 resume 与 load 绝不并发：sessionSaved 到达即 resume 落定。
    if (
      event.resume === "resumed" &&
      resumeExpectedSessionIdRef.current === null &&
      turnsRef.current.length === 0
    ) {
      const scope = {
        profileId: event.profileId,
        cwd: sessionCwd ?? null,
        sessionId: event.sessionId,
      };
      const loadSeq = transcriptLoadSeqRef.current + 1;
      transcriptLoadSeqRef.current = loadSeq;
      void (async () => {
        try {
          const events = await fetchAgentTranscript(queryClient, scope);
          if (transcriptLoadSeqRef.current !== loadSeq) return;
          if (turnsRef.current.length > 0) return;
          const loaded = mapLoadedTranscript(events);
          if (loaded.length === 0) return;
          syncTurnIdSeq(idSeq, loaded);
          seedHandledProposals(loaded);
          setTurns([...loaded]);
        } catch {
          // 读不到就留白：快照本来就是空的，不报假错。
        }
      })();
    }
    const restore = restoreRef.current;
    restoreRef.current = null;
    const attempted = resumeAttemptedRef.current;
    resumeAttemptedRef.current = null;
    if (
      restore?.sessionId &&
      attempted?.sessionId === restore.sessionId &&
      event.sessionId !== restore.sessionId &&
      turnsRef.current.length === restore.turnCount
    ) {
      // 旧会话不在了（后端给了新 id），且用户还没说过话：清掉秒开摆出来的旧 turns。
      setTurns([]);
      setNotices([]);
      clearDraft();
    }
    // 落键即真相：hint 按事件自带的 profileId 分键存放。
    setSavedSessionFor(event.profileId, {
      sessionId: event.sessionId,
      profileId: event.profileId,
      cwd: event.cwd,
    });
    // 新线程要出现在历史列表里：通知列表缓存失效。
    void queryClient.invalidateQueries({ queryKey: ["acp-session-list"] });
    // 本轮见过的用户首句覆盖原生垃圾标题（只记一次，不覆盖已有）。
    setTitleOverrides((prev) => {
      if (prev[event.sessionId]) return prev;
      const text = firstUserTextOf(turnsRef.current);
      if (!text) return prev;
      return { ...prev, [event.sessionId]: text };
    });
  };

  const newChatMutation = useMutation({
    mutationFn: async (options?: { preserveTurns?: boolean }) => {
      const preserveTurns = options?.preserveTurns ?? false;
      const profileState = useAcpProfilesStore.getState();
      const settings = clientSettingsFromStore(useAcpSettingsStore.getState());
      resumeExpectedSessionIdRef.current = null;
      resumeNoticePendingRef.current = false;
      setLastSessionResolution(null);
      setSessionBanner(null);
      // 新建对话只清当前 agent 自家键的 hint，别家的续聊不受影响。
      clearSavedSessionFor(profileState.activeProfileId);
      if (!preserveTurns) {
        setTurns([]);
        setNotices([]);
        clearDraft();
        clearPromptQueue();
        clearProposals();
      }
      setProgress(null);
      setPendingPermission(null);
      setConnectionState("connecting");
      setProgress(preserveTurns ? "正在同步 Agent 会话…" : "正在开始新对话…");
      progressOwnerRef.current = claimProgressOwner("sys:new-chat");

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
    onSuccess: async () => {
      setConnectionState("connected");
      setProgress(null);
      progressOwnerRef.current = null;
      await queryClient.invalidateQueries({ queryKey: ["acp-status"] });
    },
    onError: (error) => {
      setConnectionState("error");
      setProgress(null);
      progressOwnerRef.current = null;
      pushSystem(errorMessage(error));
    },
  });

  // 同进程切换：不断 Agent 进程，只关旧会话、resume 或新建目标。
  const switchSessionMutation = useMutation({
    mutationFn: async (options: {
      savedSession: SavedSessionHint | null;
    }) => {
      setLastSessionResolution(null);
      setSessionBanner(null);
      const profileState = useAcpProfilesStore.getState();
      const settings = clientSettingsFromStore(useAcpSettingsStore.getState());
      // 同进程切换只清目标线程所属 agent 自家键的 hint（通常就是当前 profile），
      // 别家的续聊不受影响。
      clearSavedSessionFor(
        options.savedSession?.profileId ?? profileState.activeProfileId,
      );
      setConnectionState("connecting");
      progressOwnerRef.current = claimProgressOwner("sys:switch-session");

      await acpSwitchSession(
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
          savedSession: options.savedSession,
          clientSettings: settings,
          profiles: profilesHintFromStore(
            profileState.activeProfileId,
            profileState.profiles,
          ),
        },
      );
    },
    onSuccess: async () => {
      setConnectionState("connected");
      setProgress(null);
      progressOwnerRef.current = null;
      await queryClient.invalidateQueries({ queryKey: ["acp-status"] });
    },
    onError: (error) => {
      setConnectionState("error");
      setProgress(null);
      progressOwnerRef.current = null;
      pushSystem(errorMessage(error));
    },
  });

  const switchingSession = switchSessionMutation.isPending;

  // 换 Agent = 换世界后的记账重置（原 AcpPanel 内 activeProfileId effect）。
  const lastResetProfileRef = useRef<string | null>(null);
  useEffect(() => {
    if (lastResetProfileRef.current === null) {
      // 首挂载不清：继承落盘 hint（重启自动 resume 用）；只有运行中切换才算换世界。
      lastResetProfileRef.current = activeProfileId;
      return;
    }
    if (lastResetProfileRef.current === activeProfileId) return;
    lastResetProfileRef.current = activeProfileId;
    // 换世界 epoch：connect effect 认领（见下方），本次切换必走关旧连新。
    switchEpochRef.current = { profileId: activeProfileId };
    setConnectionState("connecting");
    const switchName =
      useAcpProfilesStore
        .getState()
        .profiles.find((profile) => profile.id === activeProfileId)?.name ??
      activeProfileId;
    setProgress(`正在切换到 ${switchName}…`);
    setLastSessionResolution(null);
    setSessionBanner(null);
    resumeNoticePendingRef.current = false;
    // 只重置本轮记账：目标世界的对账基线重新 seed，各 profile hint 一律保留。
    const snapshot = readChatRestore(activeProfileId);
    restoreRef.current = snapshot
      ? {
          sessionId:
            useAcpSessionStore.getState().savedSessionFor(activeProfileId)
              ?.sessionId ?? null,
          turnCount: snapshot.turns.length,
        }
      : null;
    resumeExpectedSessionIdRef.current = null;
    transcriptLoadSeqRef.current += 1;
  }, [activeProfileId]);

  useEffect(() => {
    if (prevConnectKeyRef.current !== connectKey) {
      prevConnectKeyRef.current = connectKey;
      // 画像切换由 epoch 流显式关旧：这里再关会和新 spawn 并发，误杀新进程。
      if (switchEpochRef.current) return;
      if (sessionActive && !promptBusy) {
        void acpClose().then(() =>
          queryClient.invalidateQueries({ queryKey: ["acp-status"] }),
        );
      }
    }
  }, [connectKey, sessionActive, promptBusy, queryClient]);

  useEffect(() => {
    if (statusLoading) return;

    const profileState = useAcpProfilesStore.getState();
    // 切换 epoch 认领：只有目标画像是当前画像才认领，认领即清掉。
    const switchEpoch = switchEpochRef.current;
    const isSwitchTarget =
      switchEpoch !== null &&
      switchEpoch.profileId === profileState.activeProfileId;
    if (isSwitchTarget) {
      switchEpochRef.current = null;
    }

    if (!available) {
      setConnectionState("unavailable");
      return;
    }

    if (promptBusy) return;

    if (newChatMutation.isPending) return;

    if (switchingSession) return;

    // sessionActive 为 true 只在“当前画像有活会话”时才算已连接；
    // 切换目标无条件走关旧连新，旧 Agent 还活着也不能短路。
    if (sessionActive && !isSwitchTarget) {
      setConnectionState("connected");
      return;
    }

    let cancelled = false;
    setConnectionState("connecting");
    setProgress("正在连接 Agent…");

    const settings = clientSettingsFromStore(useAcpSettingsStore.getState());
    const session = useAcpSessionStore.getState();
    // 记下这次 connect 带没带 resume hint：sessionSaved 落定时做一次新旧对账。
    resumeAttemptedRef.current = session.savedSessionFor(
      profileState.activeProfileId,
    );
    const connectArgs = {
      profileId: profileState.activeProfileId,
      cwd: sessionCwd,
      savedSession: session.savedSessionFor(profileState.activeProfileId),
      clientSettings: settings,
      profiles: profilesHintFromStore(
        profileState.activeProfileId,
        profileState.profiles,
      ),
    };

    const runConnect = async () => {
      if (isSwitchTarget) {
        // 关旧连新同一条流里串行：关失败（本就无旧可关）不阻断，直接连新。
        try {
          await acpClose();
        } catch {
          // 旧会话本就不存在：直接连新。
        }
        if (cancelled) return;
      }
      try {
        await acpConnect((event: AcpEvent) => {
          if (cancelled) return;
          if (event.type === "progress") {
            setProgress(event.message);
          }
          if (event.type === "sessionSaved") {
            handleSessionSaved(event);
          }
        }, connectArgs);
        if (cancelled) return;
        setConnectionState("connected");
        setProgress(null);
        await queryClient.invalidateQueries({ queryKey: ["acp-status"] });
      } catch (error) {
        if (cancelled) return;
        setConnectionState("error");
        setProgress(null);
        pushSystem(errorMessage(error));
      }
    };
    void runConnect();

    return () => {
      cancelled = true;
    };
  }, [
    available,
    promptBusy,
    connectAttempt,
    connectKey,
    queryClient,
    savedSession,
    sessionActive,
    sessionCwd,
    setSavedSessionFor,
    statusLoading,
    newChatMutation.isPending,
    switchingSession,
  ]);

  const handleReconnect = () => {
    setConnectAttempt((attempt) => attempt + 1);
  };

  return {
    connectionState, progress, setProgress, progressOwnerRef,
    lastSessionResolution, setLastSessionResolution,
    sessionBanner, setSessionBanner, titleOverrides, setTitleOverrides,
    resumeExpectedSessionIdRef, resumeNoticePendingRef, transcriptLoadSeqRef,
    handleSessionSaved, newChatMutation, newChatPending: newChatMutation.isPending,
    switchSessionMutation, switchingSession, handleReconnect,
  };
}
