import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useId, useMemo, useRef, useState } from "react";

import {
  createNote,
  loadLatestAnnotationProposal,
  dismissAnnotationProposal,
} from "@/features/notes/api";
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
import { notesKey } from "@lumina/query-keys";

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
  acpDeleteSession,
  acpNewChat,
  acpPrompt,
  acpSwitchSession,
  getAcpStatus,
} from "../api";
import type { AcpTaskId, PromptImageInput } from "../api";
import {
  PASTED_IMAGE_LIMIT,
  pastedImageId,
  pickAcceptableFiles,
  readPastedFile,
  toPromptImageInput,
} from "../pastedImages";
import {
  acpQueryKeys,
  fetchAgentTranscript,
  getTranscriptCache,
  prefetchAgentSessionList,
  useAgentSessionList,
  useSyncTaskContracts,
} from "../queries";
import { profilesHintFromStore } from "@lumina/chat-ui/defaultAgentProfiles";
import {
  applyAcpEventToTurn,
  createTurn,
  pushNotice,
  syncTurnIdSeq,
  type SystemNotice,
} from "@lumina/chat-ui/chatTurns";
import {
  agentSessionListTrust,
  canSwitchHistoryConversation,
  historyThreadRows,
  resumeOutcomeNotice,
  type HistoryThreadRow,
} from "@lumina/chat-ui/conversationContext";
import { useChatUiStore } from "@lumina/chat-ui/chatUiStore";
import type { AssistantAction } from "@lumina/chat-ui/assistantBlocks";
import { buildAnchoredVideoPromptContext } from "../context";
import { mapLoadedTranscript } from "../conversationTranscript";
import { workspaceCwdFromMedia } from "@lumina/player-ui/cwd";
import {
  flushChatRestore,
  isRestorable,
  readChatRestore,
  schedulePersistChatRestore,
} from "../chatRestore";
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
  ChatImageAttachment,
  ChatTurn,
  PendingPermission,
  ResumeOutcome,
  SavedSessionHint,
  ThinkingLevel,
} from "../types";
import { useVideoPromptContext } from "../useVideoPromptContext";
import { useTypingPlaybackAnchor } from "../typingPlaybackAnchor";
import { AgentSettingsPanel } from "./AgentSettingsPanel";
import { ChatHistorySheet } from "@lumina/chat-ui/components/ChatHistorySheet";
import { ChatComposerBar, type ChatComposerBarHandle } from "./ChatComposerBar";
import { ChatShell } from "@lumina/chat-ui/components/ChatShell";
import { ChatColumn } from "@lumina/chat-ui/components/ChatShell";
import type { CompanionMode } from "@lumina/chat-ui/components/CompanionModeTabs";
import { ChatToolbar } from "./ChatToolbar";
import { CompanionHeaderPanel } from "./CompanionHeaderPanel";
import { ChatTurnList } from "./ChatTurnList";
import {
  COMPANION_TASK_LABELS,
  type CompanionTaskId,
} from "./CompanionQuickActions";
import { PermissionPrompt } from "./PermissionPrompt";

type AssistantActionHandlerDeps = {
  mediaPath: string | null;
  currentTimeMs: number;
  busy: boolean;
  seek: (positionMs: number) => Promise<void>;
  askAbout: (anchorMs: number, prompt: string) => void;
  saveNote: (input: {
    mediaPath: string;
    positionMs: number;
    body: string;
  }) => Promise<void>;
  notify: (message: string) => void;
};

export async function handleAssistantAction(
  action: AssistantAction,
  deps: AssistantActionHandlerDeps,
): Promise<void> {
  try {
    if (deps.busy) {
      deps.notify("当前回合进行中，请稍后重试");
      return;
    }
    if (!deps.mediaPath) {
      deps.notify("请先打开视频");
      return;
    }

    switch (action.type) {
      case "seek":
        await deps.seek(action.anchor.startMs);
        return;
      case "ask":
        if (typeof action.anchor.startMs !== "number") {
          deps.notify("该操作缺少时间锚点");
          return;
        }
        deps.askAbout(action.anchor.startMs, action.prompt);
        return;
      case "save-note":
        await deps.saveNote({
          mediaPath: deps.mediaPath,
          positionMs:
            typeof action.anchor.startMs === "number"
              ? action.anchor.startMs
              : deps.currentTimeMs,
          body: action.content,
        });
        deps.notify("已保存为笔记");
        return;
    }
  } catch (cause) {
    deps.notify(errorMessage(cause));
  }
}

/** Kept mounted in ChatDock after first open; hide ≠ unmount. */
function firstUserTextOf(turns: ChatTurn[]): string | null {
  const text = turns
    .find((turn) => turn.userText.trim())
    ?.userText.trim()
    .slice(0, 64);
  return text || null;
}

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

  // 任务契约唯一源头是后端提示词仓库：挂载即同步版本表，
  // 快捷任务解析不再依赖前端硬编码（离线/失败时保留 fallback）。
  useSyncTaskContracts();

  const idSeq = useState(() => ({ n: 0 }))[0];
  const listKey = useId();
  const turnListRef = useRef<HTMLDivElement | null>(null);
  const composerRef = useRef<ChatComposerBarHandle | null>(null);
  // 秒开恢复（第 1 层）：首渲染直接摆上次退出的 turns + 草稿，零网络、
  // 无 effect 闪帧。只认同 profile 的快照；对账信息进 restoreRef，
  // 由 sessionSaved 与 resume 尝试做一次新旧会话对账（第 2 层）。
  const [initialRestore] = useState(() => {
    const snapshot = readChatRestore();
    if (
      !snapshot ||
      snapshot.profileId !== useAcpProfilesStore.getState().activeProfileId
    ) {
      return null;
    }
    return snapshot;
  });
  const [draft, setDraft] = useState(initialRestore?.draft ?? "");
  // 粘贴图片附件：随下一条发送（或排队），发送/建新/切换即清空，不落盘。
  const [attachments, setAttachments] = useState<ChatImageAttachment[]>([]);
  const [turns, setTurns] = useState<ChatTurn[]>(() => initialRestore?.turns ?? []);
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
  const [historyOpen, setHistoryOpen] = useState(false);
  const resumeExpectedSessionIdRef = useRef<string | null>(null);
  const resumeNoticePendingRef = useRef(false);
  const transcriptLoadSeqRef = useRef(0);
  // 标题覆盖（本轮内存，不落盘）：原生标题常是 prompt 脚手架/文件名，
  // 用本轮见过的用户首句覆盖。文本缓存走 TanStack Query（transcript key）。
  const [titleOverrides, setTitleOverrides] = useState<Record<string, string>>(
    {},
  );
  const turnsRef = useRef<ChatTurn[]>([]);
  turnsRef.current = turns;

  // 秒开恢复（第 1 层：本地渲染缓存）：挂载瞬间先摆上次退出的 turns +
  // 草稿，零网络。只认同 profile 的快照；旧版本账本残留照旧清理，
  // 当前版本的快照与 session hint 保留，由 connect 按 resume 对账（第 2 层）。
  // restoreRef 做一次对账：尝试恢复的正是摆出来的那个旧会话、且用户还没
  // 说过话，后端却落了另一个新会话 → 清掉缓存，不把旧线程盖在新会话上。
  const restoreRef = useRef<{
    sessionId: string | null;
    turnCount: number;
  } | null>(
    initialRestore
      ? {
          sessionId:
            useAcpSessionStore.getState().savedSession?.sessionId ?? null,
          turnCount: initialRestore.turns.length,
        }
      : null,
  );
  const resumeAttemptedRef = useRef<SavedSessionHint | null>(null);
  useEffect(() => {
    try {
      window.localStorage.removeItem("lumina-acp-chat-history");
      window.localStorage.removeItem("lumina-acp-session");
    } catch {
      // 私有模式等极端环境：清不掉也不影响，内存态本来就是空的。
    }
  }, []);

  const available = statusQuery.data?.available ?? false;
  const sessionActive = statusQuery.data?.sessionActive ?? false;
  const sessionCwd = workspaceCwdFromMedia(currentFile);

  // turns + 草稿节流落盘（trailing 1.5s）：打字停一下就写，无可存内容
  // （新建对话清空后）则清快照，避免僵尸恢复。卸载时 flush。
  useEffect(() => {
    const input = {
      profileId: activeProfileId,
      cwd: sessionCwd ?? null,
      draft,
      turns,
    };
    schedulePersistChatRestore(isRestorable(input) ? input : null);
  }, [turns, draft, activeProfileId, sessionCwd]);
  useEffect(() => () => flushChatRestore(), []);  const connectKey = `${activeProfileId}:${profilesSig}:${sessionCwd ?? ""}`;

  // 换 Agent = 换世界。面板私有对话态（turns/notices/标题覆盖）在**渲染期
  // 同步**重置（React "adjust state during render" 模式，无 effect 时序、
  // 无旧内容闪帧）；跨组件共享的 savedSession 与 ref 记账留给下方 effect
  // （外部 store 的写不能放渲染期）。
  const [renderedProfileId, setRenderedProfileId] = useState(activeProfileId);
  if (renderedProfileId !== activeProfileId) {
    setRenderedProfileId(activeProfileId);
    setTurns([]);
    setNotices([]);
    setTitleOverrides({});
  }
  const lastResetProfileRef = useRef<string | null>(null);
  useEffect(() => {
    if (lastResetProfileRef.current === null) {
      // 首挂载：继承落盘的 session hint（重启自动 resume 用），不清。
      // 只有运行中切换画像才算“换世界”。
      lastResetProfileRef.current = activeProfileId;
      return;
    }
    if (lastResetProfileRef.current === activeProfileId) return;
    lastResetProfileRef.current = activeProfileId;
    clearSavedSession();
    resumeExpectedSessionIdRef.current = null;
    transcriptLoadSeqRef.current += 1;
  }, [activeProfileId, clearSavedSession]);

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
    setAttachments([]);
  };

  const handlePasteImages = (files: File[]) => {
    const { accepted, rejected } = pickAcceptableFiles(files, attachments.length);
    for (const reason of rejected) pushSystem(reason);
    if (accepted.length === 0) return;
    void (async () => {
      const next: ChatImageAttachment[] = [];
      for (const file of accepted) {
        try {
          const dataUrl = await readPastedFile(file);
          if (!dataUrl.startsWith("data:image/")) continue;
          next.push({
            id: pastedImageId(),
            mimeType: file.type.toLowerCase(),
            dataUrl,
          });
        } catch {
          pushSystem(`图片读取失败：${file.name || "未命名文件"}`);
        }
      }
      if (next.length > 0) {
        setAttachments((prev) =>
          [...prev, ...next].slice(0, PASTED_IMAGE_LIMIT),
        );
      }
    })();
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
    setCompanionMode("chat");
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

  const pushSystem = (content: string) => {
    setNotices((prev) => pushNotice(prev, idSeq, content));
  };

  // 本框内实际发起过的 Lumina 工具调用（按 toolCallId 去重）。
  // 从 turns 派生：换框/新建/载入会自动清零，无需手动维护。
  const luminaToolCalls = useMemo(() => {
    const ids = new Set<string>();
    for (const turn of turns) {
      for (const activity of turn.activities ?? []) {
        if (
          activity.kind === "tool" &&
          typeof activity.toolCallId === "string" &&
          /lumina/i.test(activity.title ?? "")
        ) {
          ids.add(activity.toolCallId);
        }
      }
    }
    return ids.size;
  }, [turns]);

  // 后端每次落定会话都会发 sessionSaved（新建/恢复成功/恢复失败转建新）。
  // 这里无条件记录，供工具栏常驻显示，后端说什么 UI 就显示什么，
  // 不再依赖“是否有人在等通知”来决定用户看不看得见。
  const [lastSessionResolution, setLastSessionResolution] = useState<{
    outcome: ResumeOutcome | "fresh";
    sessionId: string;
  } | null>(null);
  // 会话出身单槽展示（恢复/新建）：钉在对话列表顶部，不进尾部通知、
  // 不堆积。新会话落定即替换，旧的不留。
  const [sessionBanner, setSessionBanner] = useState<string | null>(null);
  // 读历史时从头看（false），现问现答时跟到底（true）。
  const [stickToEnd, setStickToEnd] = useState(true);
  const [companionMode, setCompanionMode] =
    useState<CompanionMode>("watch-feed");

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
      // 旧会话不在了（后端给了新 id），且用户还没说过话：清掉秒开摆出来的
      // 旧 turns，新会话配空白框。用户已开聊则不动（那是现线程的内容）。
      setTurns([]);
      setNotices([]);
      setDraftEmpty();
    }
    setSavedSession({
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

  const newChatMutation = useMutation({
    mutationFn: async (options?: { preserveTurns?: boolean }) => {
      const preserveTurns = options?.preserveTurns ?? false;
      resumeExpectedSessionIdRef.current = null;
      resumeNoticePendingRef.current = false;
      setLastSessionResolution(null);
      setSessionBanner(null);
      clearSavedSession();
      if (!preserveTurns) {
        setTurns([]);
        setNotices([]);
        setDraftEmpty();
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

  // 同进程切换：不断 Agent 进程，只关旧会话、resume 或新建目标。
  // UI 状态（turns/记录选择）由调用方负责，mutation 只管连接态。
  const switchSessionMutation = useMutation({
    mutationFn: async (options: {
      savedSession: SavedSessionHint | null;
    }) => {
      setLastSessionResolution(null);
      setSessionBanner(null);
      clearSavedSession();
      setConnectionState("connecting");

      const profileState = useAcpProfilesStore.getState();
      const settings = clientSettingsFromStore(useAcpSettingsStore.getState());

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
      await queryClient.invalidateQueries({ queryKey: ["acp-status"] });
    },
    onError: (error) => {
      setConnectionState("error");
      setProgress(null);
      pushSystem(errorMessage(error));
    },
  });

  const switchingSession = switchSessionMutation.isPending;
  const composerBusy =
    busy || newChatMutation.isPending || switchingSession;
  const historySwitchBlocked = !canSwitchHistoryConversation({
    busy,
    creatingSession: newChatMutation.isPending || switchingSession,
  });

  const {
    snapshot: agentSessionList,
    isError: sessionListFailed,
    isPending: sessionListPending,
  } = useAgentSessionList({
    historyOpen,
    connected: connectionState === "connected",
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
    if (connectionState !== "connected" || composerBusy) return;
    void prefetchAgentSessionList(queryClient, {
      profileId: activeProfileId,
      cwd: sessionCwd ?? null,
    });
  }, [
    connectionState,
    composerBusy,
    activeProfileId,
    sessionCwd,
    queryClient,
  ]);

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

    if (switchingSession) return;

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
    // 记下这次 connect 带没带 resume hint：sessionSaved 落定时，只对
    // “摆了缓存且确实尝试恢复同一会话”做一次新旧对账。
    resumeAttemptedRef.current = session.savedSession;

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
    savedSession,
    sessionActive,
    sessionCwd,
    setSavedSession,
    statusQuery.isLoading,
    newChatMutation.isPending,
    switchingSession,
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
        // auto 决议是系统代批，没有用户动作，静默即可；
        // 只有用户真正批准/拒绝时才值得一条系统提示。
        if (event.decision === "auto" || event.decision === "cancelled") break;
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
      images,
      taskId,
    }: {
      text: string;
      anchorPositionMs: number;
      images: ChatImageAttachment[];
      taskId?: AcpTaskId;
    }) => {
      busyRef.current = true;
      setBusy(true);
      setProgress(null);
      setPendingPermission(null);

      const turn: ChatTurn = {
        ...createTurn(idSeq, text, anchorPositionMs),
        ...(images.length > 0 ? { images: [...images] } : null),
        ...(taskId ? { shortcutTaskId: taskId } : null),
      };
      setTurns((prev) => [...prev, turn]);

      const profileState = useAcpProfilesStore.getState();
      const settings = clientSettingsFromStore(useAcpSettingsStore.getState());
      const session = useAcpSessionStore.getState();
      const promptImages: PromptImageInput[] = images.flatMap((image) => {
        const input = toPromptImageInput(image);
        return input ? [input] : [];
      });

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
            images: promptImages,
            savedSession: session.savedSession,
            clientSettings: settings,
            taskId,
            profiles: profilesHintFromStore(
              profileState.activeProfileId,
              profileState.profiles,
            ),
          },
        );
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

  const selectCompanionTask = (taskId: CompanionTaskId) => {
    if (
      !currentFile ||
      !available ||
      connectionState !== "connected" ||
      composerBusy ||
      busyRef.current ||
      runMutation.isPending
    ) {
      return;
    }

    const anchorPositionMs = consumeAnchorPositionMs();
    setStickToEnd(true);
    runMutation.mutate({
      text: COMPANION_TASK_LABELS[taskId],
      taskId,
      anchorPositionMs,
      images: [],
    });
  };

  const onAssistantAction = (action: AssistantAction) => {
    const live = usePlayerStore.getState();
    void handleAssistantAction(action, {
      mediaPath: live.currentFile,
      currentTimeMs: live.currentTimeMs,
      busy: composerBusy,
      seek: (positionMs) => usePlayerStore.getState().seek(positionMs),
      askAbout: (anchorMs, prompt) =>
        useAskAboutStore.getState().askAbout(anchorMs, prompt),
      saveNote: async ({ mediaPath, positionMs, body }) => {
        await createNote({
          mediaPath,
          positionMs,
          body,
          includeQuotes: false,
        });
        await queryClient.invalidateQueries({
          queryKey: notesKey(mediaPath),
        });
      },
      notify: pushSystem,
    });
  };

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
      images: next.images ?? [],
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
    const images = attachments;
    if (
      (!text && images.length === 0) ||
      !available ||
      connectionState !== "connected"
    ) {
      return;
    }
    const anchorPositionMs = consumeAnchorPositionMs();
    setStickToEnd(true);
    if (busy || busyRef.current || runMutation.isPending) {
      syncPromptQueue(
        enqueuePrompt(
          promptQueueRef.current,
          createQueuedPrompt(text, anchorPositionMs, undefined, images),
        ),
      );
      setDraft("");
      setAttachments([]);
      return;
    }
    setDraft("");
    setAttachments([]);
    runMutation.mutate({ text, anchorPositionMs, images });
  };

  const bargeIn = () => {
    const text = draft.trim();
    const images = attachments;
    if (
      (!text && images.length === 0) ||
      !available ||
      connectionState !== "connected"
    ) {
      return;
    }
    if (!(busy || busyRef.current || runMutation.isPending)) {
      send();
      return;
    }
    const anchorPositionMs = consumeAnchorPositionMs();
    syncPromptQueue(
      bargeInPrompt(
        promptQueueRef.current,
        createQueuedPrompt(text, anchorPositionMs, undefined, images),
      ),
    );
    setDraft("");
    setAttachments([]);
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

  // F2:新建对话必转后端新 session。以前空白框 + 活会话时直接 return，
  // 用户以为开了新的，下一问续的还是旧线程的隐藏上下文。
  const startNewChat = () => {
    if (busy || newChatMutation.isPending) return;
    setStickToEnd(false);
    if (!available) {
      setTurns([]);
      setNotices([]);
      setSessionBanner(null);
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

  // 打开历史线程：串行 resume → load。单 stdio 连接上绝不并发两件事：
  // 先定归属（记忆），落定后再独占连接读文本。失败说实话，不回退假存档。
  const loadConversation = async (sessionId: string) => {
    if (
      !canSwitchHistoryConversation({
        busy,
        creatingSession: newChatMutation.isPending,
      })
    ) {
      pushSystem("正在回答，请稍后再切换对话");
      return;
    }
    if (switchSessionMutation.isPending) {
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
    syncPromptQueue([]);
    setTurns([...cached]);
    // 读历史从头看：停掉跟随到底，滚到顶部。
    setStickToEnd(false);
    turnListRef.current?.scrollTo({ top: 0 });
    setNotices([]);
    setDraftEmpty();
    setPendingPermission(null);
    handleHistoryOpenChange(false);

    const hint: SavedSessionHint = {
      sessionId: target,
      profileId: activeProfileId,
      cwd: sessionCwd,
    };
    const currentId = useAcpSessionStore.getState().savedSession?.sessionId;
    const alreadyOnTarget = sessionActive && currentId === target;
    if (!alreadyOnTarget) {
      resumeExpectedSessionIdRef.current = target;
      resumeNoticePendingRef.current = true;
      setProgress("正在恢复该对话的 AI 记忆…");
      try {
        await switchSessionMutation.mutateAsync({ savedSession: hint });
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
        useAcpSessionStore.getState().savedSession?.sessionId !== target
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
    if (busy || newChatMutation.isPending || switchSessionMutation.isPending) {
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
      if (useAcpSessionStore.getState().savedSession?.sessionId === target) {
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
    if (busy || newChatMutation.isPending || switchSessionMutation.isPending) {
      pushSystem("正在回答，请稍后再删除对话");
      return;
    }
    if (useAcpSessionStore.getState().savedSession?.sessionId === target) {
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

  // 常驻会话真相：只反映后端最后一次 sessionSaved / 本地同线程续接，
  // 不依赖有没有人在等通知。恢复成功失败、是否新建，在这里一眼可见。
  const sessionResolutionText =
    connectionState === "connected" && lastSessionResolution
      ? lastSessionResolution.outcome === "resumed"
        ? "已恢复上次对话记忆"
        : lastSessionResolution.outcome === "fresh"
          ? "新对话"
          : "旧记忆不可用，已建新对话"
      : null;

  const statusLine = statusQuery.isLoading
    ? null
    : busy
      ? promptQueue.length > 0
        ? `回合进行中…（已排队 ${promptQueue.length} 条）`
        : "回合进行中…"
      : promptQueue.length > 0
        ? `排队 ${promptQueue.length} 条，即将发送…`
        : connectionState === "connected" && sessionCwd
        ? `工作目录：${sessionCwd}${sessionResolutionText ? ` · ${sessionResolutionText}` : ""}${luminaToolCalls > 0 ? ` · Lumina 工具已调用 ${luminaToolCalls} 次` : ""}`
        : connectionState === "connecting"
          ? null
          : hasSavedSession
            ? "已记住该对话的 AI 记忆，下次连接将自动尝试恢复"
            : (statusQuery.data?.message ?? null);

  return (
    <ChatShell data-chat-shell={listKey}>
      <div className="sticky top-0 z-20 shrink-0 bg-card">
        <ChatColumn className="shrink-0 bg-card">
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
            historyCount={historyRows.length}
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
            rows={historyRows}
            activeSessionId={savedSession?.sessionId ?? null}
            switchBlocked={historySwitchBlocked}
            loading={sessionListLoading}
            onRefresh={() =>
              void queryClient.invalidateQueries({
                queryKey: acpQueryKeys.sessionList(
                  activeProfileId,
                  sessionCwd ?? null,
                ),
              })
            }
            onClose={() => handleHistoryOpenChange(false)}
            onSelect={(sessionId) => void loadConversation(sessionId)}
            onDelete={(sessionId) => void deleteHistoryConversation(sessionId)}
            onPurgeEmpty={() => void purgeEmptyConversations()}
          />
        </ChatColumn>

        <CompanionHeaderPanel
          mode={companionMode}
          onModeChange={setCompanionMode}
          onSelectTask={selectCompanionTask}
          onAssistantAction={onAssistantAction}
          quickActionsDisabled={
            !currentFile ||
            !available ||
            connectionState !== "connected" ||
            composerBusy
          }
        />
      </div>

      <div
        ref={turnListRef}
        className="chat-scroll min-h-0 flex-1 overflow-y-auto overscroll-y-contain"
      >
        {sessionBanner ? (
          <p className="px-3 pb-1 pt-3 text-center text-[11px] text-muted-foreground">
            {sessionBanner}
          </p>
        ) : null}
        {companionMode === "watch-feed" ? (
          <div
            id="companion-panel-watch-feed-chat"
            role="region"
            aria-label="观剧流对话"
          >
            <p className="px-1 pb-1 pt-2 text-[10px] font-medium text-muted-foreground">
              当前会话
            </p>
            <ChatTurnList
              turns={turns}
              notices={notices}
              followEnd={stickToEnd}
              annotationWorkspace={sessionCwd}
              onDismissAnnotation={handleDismissAnnotation}
              onSaveAnnotation={handleSaveAnnotation}
              onAssistantAction={onAssistantAction}
              emptyHint="快捷操作的完整回答会显示在这里，也可以直接输入问题。"
            />
          </div>
        ) : (
          <div id="companion-panel-chat" role="tabpanel" aria-label="自由聊天">
            <ChatTurnList
              turns={turns}
              notices={notices}
              followEnd={stickToEnd}
              annotationWorkspace={sessionCwd}
              onDismissAnnotation={handleDismissAnnotation}
              onSaveAnnotation={handleSaveAnnotation}
              onAssistantAction={onAssistantAction}
            />
          </div>
        )}
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
        attachments={attachments}
        onPasteImages={handlePasteImages}
        onRemoveAttachment={(id) =>
          setAttachments((prev) => prev.filter((item) => item.id !== id))
        }
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

