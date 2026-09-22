import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react";
import type { QueryClient } from "@tanstack/react-query";

import { errorMessage } from "@/lib/format";
import { useAcpSessionStore } from "@lumina/chat-ui/acpSessionStore";
import { syncTurnIdSeq, type SystemNotice } from "@lumina/chat-ui/chatTurns";
import {
  agentSessionListTrust,
  canSwitchHistoryConversation,
  historyThreadRows,
  type HistoryThreadRow,
} from "@lumina/chat-ui/conversationContext";

import { acpDeleteSession } from "../api";
import type {
  ChatTurn,
  PendingPermission,
  ResumeOutcome,
  SavedSessionHint,
} from "../types";
import { mapLoadedTranscript } from "../conversationTranscript";
import {
  acpQueryKeys,
  fetchAgentTranscript,
  getTranscriptCache,
  prefetchAgentSessionList,
  useAgentSessionList,
} from "../queries";
import { firstUserTextOf } from "./useAcpConnection";

export type UseHistoryConversationInput = {
  queryClient: QueryClient;
  sessionCwd: string | undefined;
  activeProfileId: string;
  sessionActive: boolean;
  connected: boolean;
  composerBusy: boolean;
  promptBusy: boolean;
  newChatPending: boolean;
  switchingSession: boolean;
  switchSessionAsync: (options: {
    savedSession: SavedSessionHint | null;
  }) => Promise<void>;
  resumeExpectedSessionIdRef: { current: string | null };
  resumeNoticePendingRef: { current: boolean };
  transcriptLoadSeqRef: { current: number };
  idSeq: { n: number };
  turnListRef: { current: HTMLDivElement | null };
  pushSystem: (content: string) => void;
  setTurns: Dispatch<SetStateAction<ChatTurn[]>>;
  setNotices: (value: SystemNotice[]) => void;
  clearDraft: () => void;
  clearPromptQueue: () => void;
  setPendingPermission: (value: PendingPermission | null) => void;
  seedHandledProposals: (turns: ChatTurn[]) => void;
  setProgress: (value: string | null) => void;
  setLastSessionResolution: (
    value: { outcome: ResumeOutcome | "fresh"; sessionId: string } | null,
  ) => void;
  setSessionBanner: (value: string | null) => void;
  titleOverrides: Record<string, string>;
  setTitleOverrides: Dispatch<SetStateAction<Record<string, string>>>;
};

/**
 * 历史会话域（零行为搬运，原 AcpPanel 内逻辑）。
 * 收容：历史列表拉取/预热、historyRows、打开/删除/清理空占位、
 * titleOverrides 消费、stickToEnd/historyOpen。
 * AcpPanel 只留 <ChatHistorySheet> 纯展示回调。
 */
export function useHistoryConversation(input: UseHistoryConversationInput) {
  const {
    queryClient,
    sessionCwd,
    activeProfileId,
    sessionActive,
    connected,
    composerBusy,
    promptBusy,
    newChatPending,
    switchingSession,
    switchSessionAsync,
    resumeExpectedSessionIdRef,
    resumeNoticePendingRef,
    transcriptLoadSeqRef,
    idSeq,
    turnListRef,
    pushSystem,
    setTurns,
    setNotices,
    clearDraft,
    clearPromptQueue,
    setPendingPermission,
    seedHandledProposals,
    setProgress,
    setLastSessionResolution,
    setSessionBanner,
    titleOverrides,
    setTitleOverrides,
  } = input;

  const [historyOpen, setHistoryOpen] = useState(false);
  // 读历史时从头看（false），现问现答时跟到底（true）。
  const [stickToEnd, setStickToEnd] = useState(true);

  // 列表拉取失败提示一句（每个失败只提示一次）。
  const sessionListErrorNoticedRef = useRef(false);
  const noticeSessionListError = (failed: boolean) => {
    if (failed && !sessionListErrorNoticedRef.current) {
      sessionListErrorNoticedRef.current = true;
      pushSystem("历史列表暂时无法加载，可稍后重试");
    }
    if (!failed) {
      sessionListErrorNoticedRef.current = false;
    }
  };

  const {
    snapshot: agentSessionList,
    isError: sessionListFailed,
    isPending: sessionListPending,
  } = useAgentSessionList({
    historyOpen,
    connected,
    blocked: composerBusy,
    profileId: activeProfileId,
    cwd: sessionCwd ?? null,
  });
  useEffect(() => {
    noticeSessionListError(sessionListFailed);
  });
  // 加载态与空态必须长得不一样：以前没数据时“加载中”和“真没有”
  // 是同一句话，用户只能干等。pending 且无任何缓存才算加载中。
  const sessionListLoading =
    historyOpen && sessionListPending && !agentSessionList;

  const historyRows: HistoryThreadRow[] = useMemo(
    () => {
      const listScopeMatches =
        agentSessionList?.profileId === activeProfileId &&
        agentSessionList.cwd === (sessionCwd ?? null);
      const result = listScopeMatches ? agentSessionList.result : null;
      const trusted = agentSessionListTrust({
        hasData: result !== null,
        verified: result?.verified ?? false,
        truncated: result?.truncated ?? false,
      });
      // 历史真相 = 原生列表。未校验（失败/忙）时不展示，避免把残缺当全部。
      if (!trusted.canMatch) return [];
      return historyThreadRows(result?.sessions, titleOverrides);
    },
    [
      activeProfileId,
      agentSessionList,
      composerBusy,
      sessionCwd,
      titleOverrides,
    ],
  );

  const handleHistoryOpenChange = (open: boolean) => {
    // 关面板不清缓存：staleTime 内的重复打开直接秒开；
    // 新线程落定由 sessionSaved 失效缓存，作用域切换 key 天然隔离。
    setHistoryOpen(open);
  };

  // 历史列表预热：连接空闲就拉一次写进 Query 缓存，用户点开即秒开。
  // staleTime 内的重复预热是无操作，不走 IPC。
  useEffect(() => {
    if (!connected || composerBusy) return;
    void prefetchAgentSessionList(queryClient, {
      profileId: activeProfileId,
      cwd: sessionCwd ?? null,
    });
  }, [connected, composerBusy, activeProfileId, sessionCwd, queryClient]);

  const historySwitchBlocked = !canSwitchHistoryConversation({
    busy: promptBusy,
    creatingSession: newChatPending || switchingSession,
  });

  // 打开历史线程：串行 resume → load。单 stdio 连接上绝不并发两件事：
  // 先定归属（记忆），落定后再独占连接读文本。失败说实话，不回退假存档。
  const loadConversation = async (sessionId: string) => {
    if (
      !canSwitchHistoryConversation({
        busy: promptBusy,
        creatingSession: newChatPending,
      })
    ) {
      pushSystem("正在回答，请稍后再切换对话");
      return;
    }
    if (switchingSession) {
      pushSystem("正在切换对话，请稍后");
      return;
    }
    const target = sessionId.trim();
    if (!target) return;
    if (!sessionCwd) {
      pushSystem("当前没有工作目录，无法打开该对话");
      return;
    }

    const loadSeq = transcriptLoadSeqRef.current + 1;
    transcriptLoadSeqRef.current = loadSeq;
    // 秒开：Query 缓存里有就先摆（上次 load 的原生文本），回放回来再替换。
    // 缓存 miss 即空——不再有本地存档可回退，也不再编假存档。
    const scope = {
      profileId: activeProfileId,
      cwd: sessionCwd,
      sessionId: target,
    };
    let cached: ChatTurn[] = [];
    try {
      cached = mapLoadedTranscript(getTranscriptCache(queryClient, scope));
    } catch {
      cached = [];
    }
    syncTurnIdSeq(idSeq, cached);
    seedHandledProposals(cached);
    clearPromptQueue();
    setTurns([...cached]);
    // 读历史从头看：停掉跟随到底，滚到顶部。
    setStickToEnd(false);
    turnListRef.current?.scrollTo({ top: 0 });
    setNotices([]);
    clearDraft();
    setPendingPermission(null);
    handleHistoryOpenChange(false);

    const hint: SavedSessionHint = {
      sessionId: target,
      profileId: activeProfileId,
      cwd: sessionCwd,
    };
    const currentId = useAcpSessionStore
      .getState()
      .savedSessionFor(activeProfileId)?.sessionId;
    const alreadyOnTarget = sessionActive && currentId === target;
    if (!alreadyOnTarget) {
      resumeExpectedSessionIdRef.current = target;
      resumeNoticePendingRef.current = true;
      setProgress("正在恢复该对话的 AI 记忆…");
      try {
        await switchSessionAsync({ savedSession: hint });
      } catch {
        // 归属未定（busy 等原文已由 mutation onError 报出），不读文本。
        if (transcriptLoadSeqRef.current !== loadSeq) return;
        setProgress(null);
        return;
      }
      if (transcriptLoadSeqRef.current !== loadSeq) return;
      // resume 若失败，后端会新建会话并把归属切走：此时再去 load 旧线程
      // 必然失败，直接说实话，不浪费一次回放。
      if (
        useAcpSessionStore.getState().savedSessionFor(activeProfileId)
          ?.sessionId !== target
      ) {
        pushSystem(
          cached.length > 0
            ? "该对话的 AI 记忆已不存在，仅可查看缓存"
            : "该对话的 AI 记忆已不存在，无文本可载入",
        );
        setProgress(null);
        return;
      }
    } else {
      setLastSessionResolution({ outcome: "resumed", sessionId: target });
      setSessionBanner(`已恢复该对话的 AI 记忆（${target.slice(0, 8)}）`);
    }

    setProgress("正在载入该对话的真实记录…");
    try {
      // 串行：归属落定后才读，写进 transcript key 缓存（staleTime 0，
      // 每次打开都重新回放，线程可能被其它客户端续写）。
      const events = await fetchAgentTranscript(queryClient, scope);
      if (transcriptLoadSeqRef.current !== loadSeq) return;
      let mapped: ChatTurn[] = [];
      try {
        mapped = mapLoadedTranscript(events);
      } catch {
        mapped = [];
      }
      if (mapped.length === 0) {
        pushSystem(
          cached.length > 0
            ? "远端暂无可显示的文本，已保留缓存"
            : "未能载入该对话的文本，可稍后重试",
        );
        setProgress(null);
        return;
      }
      syncTurnIdSeq(idSeq, mapped);
      seedHandledProposals(mapped);
      const title = firstUserTextOf(mapped);
      if (title) {
        setTitleOverrides((prev) =>
          prev[target] ? prev : { ...prev, [target]: title },
        );
      }
      setTurns(mapped);
      turnListRef.current?.scrollTo({ top: 0 });
      setProgress(null);
    } catch (error) {
      if (transcriptLoadSeqRef.current !== loadSeq) return;
      // 超时（125s 前端计时）与后端秒回的失败是两回事，不共用一句话，
      // 否则下次看日志对不上（后端秒回失败时前端却报超时）。
      const timedOut = error instanceof Error && error.message === "timeout";
      let message: string;
      if (cached.length > 0) {
        message = timedOut ? "载入超时，已保留缓存" : "载入失败，已保留缓存";
      } else {
        message = timedOut
          ? "载入该对话的文本超时，可稍后重试"
          : "未能载入该对话的文本，可稍后重试";
      }
      pushSystem(message);
      setProgress(null);
    }
  };

  // 批量清理空占位：无原生标题 + 自家 kind（失败回退造的空线程）。
  // 逐个真删（后端同样守 busy/在用）；单次确认——空占位零价值，
  // 逐条确认纯属添堵；有内容的行不在此列，走行级删除键。
  const purgeEmptyConversations = async () => {
    const targets = historyRows
      .filter((row) => row.isEmptyCandidate)
      .map((row) => row.sessionId);
    if (targets.length === 0) return;
    if (promptBusy || newChatPending || switchingSession) {
      pushSystem("正在回答，请稍后再清理空对话");
      return;
    }
    if (
      !window.confirm(
        `删除 ${targets.length} 个无标题的空占位对话吗？远端线程将被永久删除。`,
      )
    ) {
      return;
    }
    const removed: string[] = [];
    for (const target of targets) {
      if (
        useAcpSessionStore.getState().savedSessionFor(activeProfileId)
          ?.sessionId === target
      ) {
        continue;
      }
      try {
        await acpDeleteSession({ profileId: activeProfileId, sessionId: target });
        removed.push(target);
      } catch (error) {
        pushSystem(errorMessage(error));
        break;
      }
    }
    for (const sessionId of removed) {
      void queryClient.removeQueries({
        queryKey: acpQueryKeys.transcript(
          activeProfileId,
          sessionCwd ?? null,
          sessionId,
        ),
      });
    }
    setTitleOverrides((prev) => {
      if (!removed.some((id) => prev[id])) return prev;
      const next = { ...prev };
      for (const id of removed) delete next[id];
      return next;
    });
    await queryClient.invalidateQueries({
      queryKey: acpQueryKeys.sessionList(activeProfileId, sessionCwd ?? null),
    });
    if (removed.length > 0) {
      pushSystem(`已清理 ${removed.length} 个空对话`);
    }
  };

  // 真删除：远端线程永久消失。忙时不删（同根 stdin），正在使用的不删，
  // 删前确认（误删不可恢复）。成功后清掉该线程的文本缓存并刷新列表。
  const deleteHistoryConversation = async (sessionId: string) => {
    const target = sessionId.trim();
    if (!target) return;
    if (promptBusy || newChatPending || switchingSession) {
      pushSystem("正在回答，请稍后再删除对话");
      return;
    }
    if (
      useAcpSessionStore.getState().savedSessionFor(activeProfileId)
        ?.sessionId === target
    ) {
      pushSystem("不能删除正在使用的对话，先切换到其他对话");
      return;
    }
    if (!window.confirm("删除该对话吗？远端线程将被永久删除。")) return;
    try {
      await acpDeleteSession({ profileId: activeProfileId, sessionId: target });
      void queryClient.removeQueries({
        queryKey: acpQueryKeys.transcript(
          activeProfileId,
          sessionCwd ?? null,
          target,
        ),
      });
      await queryClient.invalidateQueries({
        queryKey: acpQueryKeys.sessionList(activeProfileId, sessionCwd ?? null),
      });
      setTitleOverrides((prev) => {
        if (!prev[target]) return prev;
        const next = { ...prev };
        delete next[target];
        return next;
      });
      pushSystem("已删除该对话");
    } catch (error) {
      pushSystem(errorMessage(error));
    }
  };

  return {
    historyOpen,
    handleHistoryOpenChange,
    historyRows,
    sessionListLoading,
    stickToEnd,
    setStickToEnd,
    historySwitchBlocked,
    loadConversation,
    deleteHistoryConversation,
    purgeEmptyConversations,
  };
}

export type HistoryConversation = ReturnType<typeof useHistoryConversation>;
