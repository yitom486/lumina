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
  acpPrompt,
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
  useSyncTaskContracts,
} from "../queries";
import { profilesHintFromStore } from "@lumina/chat-ui/defaultAgentProfiles";
import {
  applyAcpEventToTurn,
  createTurn,
  pushNotice,
  type SystemNotice,
} from "@lumina/chat-ui/chatTurns";
import { useChatUiStore } from "@lumina/chat-ui/chatUiStore";
import type { AssistantAction } from "@lumina/chat-ui/assistantBlocks";
import { buildAnchoredVideoPromptContext } from "../context";
import { workspaceCwdFromMedia } from "@lumina/player-ui/cwd";
import {
  flushChatRestore,
  isRestorable,
  readChatRestore,
  scheduleClearChatRestore,
  schedulePersistChatRestore,
} from "../chatRestore";
import {
  acceptsProgressEvent,
  claimProgressOwner,
  sealProgressOwner,
} from "../progressOwner";
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
  AcpEvent,
  ChatImageAttachment,
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
import { CompanionHeaderPanel } from "./CompanionHeaderPanel";
import { ChatTurnList } from "./ChatTurnList";
import {
  COMPANION_TASK_LABELS,
  type CompanionTaskId,
} from "./CompanionQuickActions";
import { PermissionPrompt } from "./PermissionPrompt";
import { useAcpConnection } from "./useAcpConnection";
import { useHistoryConversation } from "./useHistoryConversation";

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
        // 提问不一定有时间戳（如观众问题）：缺省取当前播放位置，和存批注同策略。
        deps.askAbout(
          typeof action.anchor?.startMs === "number"
            ? action.anchor.startMs
            : deps.currentTimeMs,
          action.prompt,
        );
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
export function AcpPanel() {
  const queryClient = useQueryClient();
  const currentFile = usePlayerStore((s) => s.currentFile);
  const thinkingLevel = useAcpSettingsStore((s) => s.thinkingLevel);
  const activeProfileId = useAcpProfilesStore((s) => s.activeProfileId);
  const profilesSig = useAcpProfilesStore((s) => profilesSignature(s.profiles));
  // 会话 hint 按 agent 分键：当前 profile 只读自家键。切到 claude/cursor
  // 时 codex 的 hint 原样躺在 "codex" 键里，切回即 resume，谁也不删谁的。
  const savedSession = useAcpSessionStore(
    (s) => s.savedSessions[activeProfileId] ?? null,
  );
  const hasSavedSession = savedSession !== null;
  const clearSavedSessionFor = useAcpSessionStore(
    (s) => s.clearSavedSessionFor,
  );
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
  // 无 effect 闪帧。只认当前 profile 自家键的快照；对账信息进 restoreRef，
  // 由 sessionSaved 与 resume 尝试做一次新旧会话对账（第 2 层）。
  const [initialRestore] = useState(() => {
    const mountProfileId = useAcpProfilesStore.getState().activeProfileId;
    return readChatRestore(mountProfileId);
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
  const seedHandledProposals = (nextTurns: ChatTurn[]) => {
    const bindings = seedProposalBindingsFromTurns(nextTurns);
    handledProposalIdsRef.current = bindings.handledProposalIds;
    proposalTurnByIdRef.current = bindings.proposalTurnById;
    proposalBindingsRef.current = bindings;
  };
  const [quickNoteOpen, setQuickNoteOpen] = useState(false);
  const turnsRef = useRef<ChatTurn[]>([]);
  turnsRef.current = turns;

  const available = statusQuery.data?.available ?? false;
  const sessionActive = statusQuery.data?.sessionActive ?? false;
  const sessionCwd = workspaceCwdFromMedia(currentFile);

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

  const pushSystem = (content: string) => {
    setNotices((prev) => pushNotice(prev, idSeq, content));
  };

  const syncPromptQueue = (next: QueuedPrompt[]) => {
    promptQueueRef.current = next;
    setPromptQueue(next);
  };

  // 连接域：connect/close、会话落定对账、新建/切换会话 mutation。
  const connection = useAcpConnection({
    queryClient,
    sessionCwd,
    profilesSig,
    available,
    sessionActive,
    statusLoading: statusQuery.isLoading,
    savedSession,
    promptBusy: busy,
    turnsRef,
    idSeq,
    pushSystem,
    setTurns,
    setNotices,
    clearDraft: setDraftEmpty,
    clearPromptQueue: () => syncPromptQueue([]),
    clearProposals: () => {
      handledProposalIdsRef.current.clear();
      proposalTurnByIdRef.current.clear();
      proposeToolCallIdsRef.current.clear();
      proposalBindingsRef.current = createProposalBindings();
    },
    setPendingPermission,
    seedHandledProposals,
    initialRestore,
  });

  const composerBusy =
    busy || connection.newChatPending || connection.switchingSession;

  // 历史会话域：列表、打开/删除/清理、historyRows。
  const history = useHistoryConversation({
    queryClient,
    sessionCwd,
    activeProfileId,
    sessionActive,
    connected: connection.connectionState === "connected",
    composerBusy,
    promptBusy: busy,
    newChatPending: connection.newChatPending,
    switchingSession: connection.switchingSession,
    switchSessionAsync: (options) =>
      connection.switchSessionMutation.mutateAsync(options),
    resumeExpectedSessionIdRef: connection.resumeExpectedSessionIdRef,
    resumeNoticePendingRef: connection.resumeNoticePendingRef,
    transcriptLoadSeqRef: connection.transcriptLoadSeqRef,
    idSeq,
    turnListRef,
    pushSystem,
    setTurns,
    setNotices,
    clearDraft: setDraftEmpty,
    clearPromptQueue: () => syncPromptQueue([]),
    setPendingPermission,
    seedHandledProposals,
    setProgress: connection.setProgress,
    setLastSessionResolution: connection.setLastSessionResolution,
    setSessionBanner: connection.setSessionBanner,
    titleOverrides: connection.titleOverrides,
    setTitleOverrides: connection.setTitleOverrides,
  });

  useEffect(() => {
    try {
      window.localStorage.removeItem("lumina-acp-chat-history");
      window.localStorage.removeItem("lumina-acp-session");
    } catch {
      // 私有模式等极端环境：清不掉也不影响，内存态本来就是空的。
    }
  }, []);

  // turns + 草稿节流落盘（trailing 1.5s）：打字停一下就写，无可存内容
  // （新建对话清空后）则清自家键的快照，避免僵尸恢复。卸载时 flush。
  // 落盘按 profile 分键分槽：快切 profile 时旧世界的 trailing 写照样落回
  // 旧键，不会被顶掉也不会串键。
  useEffect(() => {
    const input = {
      profileId: activeProfileId,
      cwd: sessionCwd ?? null,
      draft,
      turns,
    };
    if (isRestorable(input)) {
      schedulePersistChatRestore(input);
    } else {
      scheduleClearChatRestore(activeProfileId);
    }
  }, [turns, draft, activeProfileId, sessionCwd]);
  useEffect(() => () => flushChatRestore(), []);

  // 换 Agent = 换世界。面板私有对话态（turns/草稿/notices/标题覆盖）在
  // **渲染期同步**重置并摆上目标世界的快照（React "adjust state during
  // render" 模式，无 effect 时序、无旧内容闪帧）；各世界的 hint 与快照按
  // profile 分键保留，切回即 resume——本路径不删任何落盘记忆。
  // 跨组件共享的 ref 记账留给下方 effect（外部 store 的写不能放渲染期，
  // 但这里本来就没有 store 写：hint 的读写全在各 mutations 里显式带 profile）。
  const [renderedProfileId, setRenderedProfileId] = useState(activeProfileId);
  if (renderedProfileId !== activeProfileId) {
    setRenderedProfileId(activeProfileId);
    const snapshot = readChatRestore(activeProfileId);
    setTurns(snapshot?.turns ?? []);
    setDraft(snapshot?.draft ?? "");
    setNotices([]);
    connection.setTitleOverrides({});
  }

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

  useEffect(() => {
    setAcpResponding(composerBusy);
    return () => setAcpResponding(false);
  }, [composerBusy, setAcpResponding]);

  // 表头一键换 Agent：与“Agent 设置”里的下拉同规则——画像归属各家，
  // 切换即清模型选择（codex 的模型绝不漏进 cursor），各家会话记忆
  // 按 profile 分键保留，切回即 resume（见 session 隔离）。
  // 唯一入口恒走 switchActiveProfileId（裸 setActiveProfileId 只留给
  // 非切换场景）；ChatToolbar 下拉经本回调间接受益，无需直连 store。
  const handleSwitchProfile = (id: string) => {
    // busy/composerBusy 守卫保留：回合进行中（对标隔壁 prompting）拒绝切换，
    // promptQueue 非空即隐含 busy，本清队列只是防竞态的兜底。
    if (id === activeProfileId || composerBusy) return;
    // 切换三清（对标隔壁 99-101 行清空旧家遗留）：旧世界的瞬态绝不漏进新世界。
    // permission 弹窗、progress 行先清；progressOwner 做 seal（迟到事件随后即废）。
    setPendingPermission(null);
    connection.setProgress(null);
    if (connection.progressOwnerRef.current) {
      connection.progressOwnerRef.current = sealProgressOwner(
        connection.progressOwnerRef.current,
        connection.progressOwnerRef.current.turnId,
      );
    }
    // 排队与打断锁、proposal 三本账、粘贴附件全部清空。
    drainLockRef.current = false;
    syncPromptQueue([]);
    handledProposalIdsRef.current.clear();
    proposalTurnByIdRef.current.clear();
    proposeToolCallIdsRef.current.clear();
    proposalBindingsRef.current = createProposalBindings();
    setAttachments([]);
    useAcpProfilesStore.getState().switchActiveProfileId(id);
    // 模型选择按 profile 隔离：id 即 agent 专属，绝不跨家。permissionMode 与
    // thinkingLevel 保持现状不变（全局偏好，切 profile 是否保留待产品确认，
    // 不擅自改语义），只清模型二元组。
    useAcpSettingsStore
      .getState()
      .patchSettings({ modelId: "", reasoningEffort: "" });
    // savedSessions hint 与 chatRestore 快照按 profile 分键故意保留：
    // 切回即 resume，本路径绝不删别家键（也不调 scheduleClear/discard）。
    void queryClient.invalidateQueries({ queryKey: ["acp-status"] });
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
        // 进度行只有归属者能写：迟到/串台的 started 不许覆盖新一轮的文案。
        if (acceptsProgressEvent(connection.progressOwnerRef.current, turnId)) {
          connection.setProgress("会话已开始");
        }
        break;
      case "progress":
        if (acceptsProgressEvent(connection.progressOwnerRef.current, turnId)) {
          connection.setProgress(event.message);
        }
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
        connection.handleSessionSaved(event);
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
        if (event.type === "finished" || event.type === "failed") {
          // 气泡更新上面已做；清行 + seal 只有归属者能做，且只做一次。
          // 迟到的 finished 不许清掉新一轮的行，finished 之后的迟到
          // progress 会被 seal 挡掉（“回答结束还在发送”的根因）。
          if (acceptsProgressEvent(connection.progressOwnerRef.current, turnId)) {
            connection.progressOwnerRef.current = sealProgressOwner(
              connection.progressOwnerRef.current,
              turnId,
            );
            connection.setProgress(null);
          }
          void queryClient.invalidateQueries({ queryKey: ["acp-status"] });
        }
        break;
      }
    }
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
      connection.setProgress(null);
      setPendingPermission(null);

      const turn: ChatTurn = {
        ...createTurn(idSeq, text, anchorPositionMs),
        ...(images.length > 0 ? { images: [...images] } : null),
        ...(taskId ? { shortcutTaskId: taskId } : null),
      };
      // 本轮拥有进度行：此后的 started/progress/finished 才配写行清行。
      connection.progressOwnerRef.current = claimProgressOwner(turn.id);
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
            savedSession: session.savedSessionFor(profileState.activeProfileId),
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
      connection.setProgress(null);
      connection.progressOwnerRef.current = null;
      void queryClient.invalidateQueries({ queryKey: ["acp-status"] });
      window.setTimeout(() => composerRef.current?.focusInput(), 0);
      window.setTimeout(() => composerRef.current?.focusInput(), 120);
    },
  });

  const selectCompanionTask = (taskId: CompanionTaskId) => {
    if (
      !currentFile ||
      !available ||
      connection.connectionState !== "connected" ||
      composerBusy ||
      busyRef.current ||
      runMutation.isPending
    ) {
      return;
    }

    const anchorPositionMs = consumeAnchorPositionMs();
    history.setStickToEnd(true);
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
      connection.newChatPending
    ) {
      return;
    }
    if (!available || connection.connectionState !== "connected") return;
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
    if (!available || connection.connectionState !== "connected") return;
    if (promptQueue.length === 0) return;
    launchNextQueuedPrompt();
  }, [
    available,
    busy,
    composerBusy,
    connection.connectionState,
    promptQueue,
  ]);

  const send = () => {
    const text = draft.trim();
    const images = attachments;
    if (
      (!text && images.length === 0) ||
      !available ||
      connection.connectionState !== "connected"
    ) {
      return;
    }
    const anchorPositionMs = consumeAnchorPositionMs();
    history.setStickToEnd(true);
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
      connection.connectionState !== "connected"
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
    connection.setProgress("正在取消…");
    void acpCancel();
    if (queued > 0) {
      pushSystem(`已取消当前回合，将继续发送排队中的 ${queued} 条`);
    }
  };

  // F2:新建对话必转后端新 session。以前空白框 + 活会话时直接 return，
  // 用户以为开了新的，下一问续的还是旧线程的隐藏上下文。
  const startNewChat = () => {
    if (busy || connection.newChatPending) return;
    history.setStickToEnd(false);
    if (!available) {
      setTurns([]);
      setNotices([]);
      connection.setSessionBanner(null);
      setDraftEmpty();
      connection.setProgress(null);
      connection.progressOwnerRef.current = null;
      setPendingPermission(null);
      syncPromptQueue([]);
      // 后端不可用只清当前 agent 自家键，别家的续聊不受影响。
      clearSavedSessionFor(activeProfileId);
      return;
    }
    syncPromptQueue([]);
    // mutate 级 onSuccess 与 mutation 级 onSuccess 都会触发（先后来后），
    // 这里只补原来 onSuccess 里的关历史面板逻辑，连接态逻辑仍在 hook 内。
    connection.newChatMutation.mutate(undefined, {
      onSuccess: (_data, options) => {
        if (!options?.preserveTurns) {
          history.handleHistoryOpenChange(false);
        }
      },
    });
  };

  // 常驻会话真相：只反映后端最后一次 sessionSaved / 本地同线程续接，
  // 不依赖有没有人在等通知。恢复成功失败、是否新建，在这里一眼可见。
  const sessionResolutionText =
    connection.connectionState === "connected" && connection.lastSessionResolution
      ? connection.lastSessionResolution.outcome === "resumed"
        ? "已恢复上次对话记忆"
        : connection.lastSessionResolution.outcome === "fresh"
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
        : connection.connectionState === "connected" && sessionCwd
        ? `工作目录：${sessionCwd}${sessionResolutionText ? ` · ${sessionResolutionText}` : ""}${luminaToolCalls > 0 ? ` · Lumina 工具已调用 ${luminaToolCalls} 次` : ""}`
        : connection.connectionState === "connecting"
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
            connectionState={connection.connectionState}
            profiles={statusQuery.data?.profiles}
            activeProfileId={activeProfileId}
            onSwitchProfile={handleSwitchProfile}
            statusLine={statusLine}
            statusError={
              statusQuery.isError ? errorMessage(statusQuery.error) : null
            }
            loading={statusQuery.isLoading}
            busy={composerBusy}
            historyCount={history.historyRows.length}
            onNewChat={startNewChat}
            onOpenHistory={() => {
              setQuickNoteOpen(false);
              history.handleHistoryOpenChange(!history.historyOpen);
            }}
            quickNoteDisabled={!currentFile}
            quickNoteOpen={quickNoteOpen}
            onQuickNoteOpenChange={(open) => {
              if (open) history.handleHistoryOpenChange(false);
              setQuickNoteOpen(open);
            }}
            onQuickNoteSaved={() => pushSystem("批注已保存")}
            onReconnect={connection.handleReconnect}
          />
          <ChatHistorySheet
            open={history.historyOpen}
            rows={history.historyRows}
            activeSessionId={savedSession?.sessionId ?? null}
            switchBlocked={history.historySwitchBlocked}
            loading={history.sessionListLoading}
            onRefresh={() =>
              void queryClient.invalidateQueries({
                queryKey: acpQueryKeys.sessionList(
                  activeProfileId,
                  sessionCwd ?? null,
                ),
              })
            }
            onClose={() => history.handleHistoryOpenChange(false)}
            onSelect={(sessionId) => void history.loadConversation(sessionId)}
            onDelete={(sessionId) => void history.deleteHistoryConversation(sessionId)}
            onPurgeEmpty={() => void history.purgeEmptyConversations()}
          />
        </ChatColumn>

        <CompanionHeaderPanel
          onSelectTask={selectCompanionTask}
          onAssistantAction={onAssistantAction}
          quickActionsDisabled={
            !currentFile ||
            !available ||
            connection.connectionState !== "connected" ||
            composerBusy
          }
        />
      </div>

      <div
        ref={turnListRef}
        className="chat-scroll min-h-0 flex-1 overflow-y-auto overscroll-y-contain"
      >
        {connection.sessionBanner ? (
          <p className="px-3 pb-1 pt-3 text-center text-[11px] text-muted-foreground">
            {connection.sessionBanner}
          </p>
        ) : null}
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
            followEnd={history.stickToEnd}
            annotationWorkspace={sessionCwd}
            onDismissAnnotation={handleDismissAnnotation}
            onSaveAnnotation={handleSaveAnnotation}
            onAssistantAction={onAssistantAction}
            emptyHint="快捷操作的完整回答会显示在这里，也可以直接输入问题。"
          />
        </div>
      </div>

      {pendingPermission ? (
        <PermissionPrompt
          pending={pendingPermission}
          onDone={() => setPendingPermission(null)}
        />
      ) : null}

      {connection.progress ? (
        <ChatColumn className="shrink-0 pb-1 text-[11px] text-muted-foreground">
          {connection.progress}
        </ChatColumn>
      ) : null}

      <ChatComposerBar
        ref={composerRef}
        value={draft}
        disabled={!available || connection.connectionState !== "connected"}
        busy={composerBusy}
        status={statusQuery.data}
        sessionConnected={connection.connectionState === "connected"}
        placeholder={
          !available
            ? "请展开下方 Agent 设置并配置可用的 Agent"
            : connection.newChatPending || connection.connectionState === "connecting"
              ? "正在连接 Agent…"
              : connection.connectionState === "error"
                ? "连接失败，请点击上方「重连」"
                : connection.connectionState === "idle"
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
        sessionConnected={connection.connectionState === "connected"}
        sessionCwd={sessionCwd}
      />
    </ChatShell>
  );
}
