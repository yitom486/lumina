import {
  keepPreviousData,
  useQuery,
  type QueryClient,
} from "@tanstack/react-query";

import {
  acpLoadSession,
  listAcpAgentSessions,
  type LoadedTranscriptEvent,
} from "./api";
import type { AgentSessionListResult } from "./types";

/** Agent 会话列表快照（单个 profile+cwd 作用域）。 */
export type HistorySessionListSnapshot = {
  result: AgentSessionListResult;
  profileId: string;
  cwd: string | null;
};

/** 跨文件复用 Query key 必须走这里，不手写字面量。 */
export const acpQueryKeys = {
  sessionList: (profileId: string, cwd: string | null) =>
    ["acp-session-list", profileId, cwd] as const,
  transcript: (profileId: string, cwd: string | null, sessionId: string) =>
    ["acp-transcript", profileId, cwd, sessionId] as const,
};

const UNVERIFIED_EMPTY_RESULT: AgentSessionListResult = {
  verified: false,
  sessions: [],
  truncated: false,
};

/** 列表缓存有效期：预热后 30s 内重复打开不走 IPC，直接秒开。 */
const SESSION_LIST_STALE_MS = 30_000;

/** 列表 Query 的 key+fn 单一来源：hook、预热、刷新都走这里。 */
export function sessionListQuery(profileId: string, cwd: string | null) {
  return {
    queryKey: acpQueryKeys.sessionList(profileId, cwd),
    queryFn: async (): Promise<HistorySessionListSnapshot> => {
      const result = await listAcpAgentSessions(profileId, cwd);
      return { result, profileId, cwd };
    },
  } as const;
}

/**
 * 历史面板打开时拉取 Agent 会话列表，替代手写的
 * useState + requested 记账 + 序号防串话。
 *
 * - enabled  целиком表达旧的 shouldRequest 条件（打开 + 已连接 + 非忙）；
 * - key 自带作用域，切目录/画像自动隔离，串话由 Query 机制处理；
 * - 关面板不自动清缓存，调用方按旧语义 removeQueries 即可；
 * - 失败映射为空的 unverified 快照（旧 catch 分支语义），调用方据此降级。
 */
export function useAgentSessionList(input: {
  historyOpen: boolean;
  connected: boolean;
  blocked: boolean;
  profileId: string;
  cwd: string | null;
}) {
  const { historyOpen, connected, blocked, profileId, cwd } = input;
  const query = useQuery({
    ...sessionListQuery(profileId, cwd),
    enabled: historyOpen && connected && !blocked,
    placeholderData: keepPreviousData,
    retry: false,
    refetchOnWindowFocus: false,
    refetchOnReconnect: false,
    staleTime: SESSION_LIST_STALE_MS,
  });
  const snapshot: HistorySessionListSnapshot | null =
    query.data ??
    (query.isError
      ? {
          result: { ...UNVERIFIED_EMPTY_RESULT },
          profileId,
          cwd,
        }
      : null);
  return {
    snapshot,
    isError: query.isError,
    isPending: query.isPending && !query.data,
  };
}

/**
 * 连接空闲即预热列表：用户点开历史时缓存命中，直接秒开。
 * staleTime 内重复调用是无操作（不走 IPC），失败也不抛（打开时再拉）。
 */
export function prefetchAgentSessionList(
  queryClient: QueryClient,
  input: { profileId: string; cwd: string | null },
) {
  return queryClient.prefetchQuery({
    ...sessionListQuery(input.profileId, input.cwd),
    staleTime: SESSION_LIST_STALE_MS,
  });
}

// 后端 session/load 预算 120s（大线程流式回放）；前端只多不少，
// 超时才判失败，不再提前认输。
const LOAD_SESSION_TIMEOUT_MS = 125_000;

function loadSessionWithTimeout(
  profileId: string,
  sessionId: string,
  cwd: string | null,
): Promise<LoadedTranscriptEvent[]> {
  return new Promise<Awaited<ReturnType<typeof acpLoadSession>>>(
    (resolve, reject) => {
      const timer = window.setTimeout(
        () => reject(new Error("timeout")),
        LOAD_SESSION_TIMEOUT_MS,
      );
      acpLoadSession({ profileId, sessionId, cwd }).then(
        (events) => {
          window.clearTimeout(timer);
          resolve(events);
        },
        (error) => {
          window.clearTimeout(timer);
          reject(error);
        },
      );
    },
  );
}

/**
 * 拉取原生线程文本并写入 Query 缓存。调用方串行调用
 * （resume 落定之后），不在 enabled 里抢连接：
 * 单 stdio 上 resume 与 load 绝不并发。
 *
 * staleTime 0：每次打开都重新回放（线程可能被其它客户端续写），
 * 秒开靠调用方先读 getTranscriptCache。
 */
export function fetchAgentTranscript(
  queryClient: QueryClient,
  input: { profileId: string; cwd: string | null; sessionId: string },
): Promise<LoadedTranscriptEvent[]> {
  return queryClient.fetchQuery({
    queryKey: acpQueryKeys.transcript(
      input.profileId,
      input.cwd,
      input.sessionId,
    ),
    queryFn: () =>
      loadSessionWithTimeout(input.profileId, input.sessionId, input.cwd),
    staleTime: 0,
    retry: false,
  });
}

/** 打开历史时先摆缓存秒开，fetch 回来再替换；无缓存即空。 */
export function getTranscriptCache(
  queryClient: QueryClient,
  input: { profileId: string; cwd: string | null; sessionId: string },
): LoadedTranscriptEvent[] {
  return (
    queryClient.getQueryData<LoadedTranscriptEvent[]>(
      acpQueryKeys.transcript(input.profileId, input.cwd, input.sessionId),
    ) ?? []
  );
}
