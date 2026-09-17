import { keepPreviousData, useQuery } from "@tanstack/react-query";

import { listAcpAgentSessions } from "./api";
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
};

const UNVERIFIED_EMPTY_RESULT: AgentSessionListResult = {
  verified: false,
  sessions: [],
  truncated: false,
};

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
    queryKey: acpQueryKeys.sessionList(profileId, cwd),
    queryFn: async (): Promise<HistorySessionListSnapshot> => {
      const result = await listAcpAgentSessions(cwd);
      return { result, profileId, cwd };
    },
    enabled: historyOpen && connected && !blocked,
    placeholderData: keepPreviousData,
    retry: false,
    refetchOnWindowFocus: false,
    refetchOnReconnect: false,
    staleTime: 0,
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
  return { snapshot, isError: query.isError };
}
