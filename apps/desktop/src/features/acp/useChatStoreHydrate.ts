import { useEffect, useRef } from "react";
import type { QueryClient } from "@tanstack/react-query";

import { errorMessage } from "@/lib/format";
import { useAcpSessionStore } from "@lumina/chat-ui/acpSessionStore";

import {
  backfillSnapshotToStore,
  fetchHintFromStore,
  fetchSnapshotFromStore,
  migrateLegacyChatStoreOnce,
  readChatRestore,
  type ChatRestoreSnapshot,
} from "./chatRestore";
import { useChatSnapshotStore } from "./chatSnapshotStore";
import { chatStoreQueryKeys } from "./queries";

export type HydratedSnapshotHandler = (
  snapshot: ChatRestoreSnapshot,
) => void;

/**
 * SQLite 快照仓水合 hook（B3 SQLite only）。
 *
 * 首渲染不阻塞：组件挂载瞬间读的一律是同步内存镜像，
 * 这里只在 effect 里异步回填，读顺序内存 → SQLite → 空：
 * - 先跑一次 `migrateLegacyChatStoreOnce`（DB 就绪证明；失败不阻塞，
 *   下次挂载重试；备层只在这里一次性读取，标记置位后不再读写备层）；
 * - hint：DB 命中且与内存不一致才覆盖内存（offline 备保留），
 *   miss 则保留内存（重启靠 zustand persist 回来的备）。
 * - snapshot：DB 命中 → 写内存镜像 + Query 缓存，并回调调用方
 *   （调用方只在面板仍空白时 reseeding，绝不覆盖用户已输入）；
 *   DB miss 且未迁移 → 读备层一次 → 写内存镜像 + 回填 DB（收敛），再回调；
 *   已迁移后 DB miss 即空（无僵尸恢复）。
 *
 * 切换路径 epoch/seq 守卫：effect 每次因 profile/session 变化重跑都
 * 递增 seq，过期回包在落定前全部丢弃，快切 A→B→A 不会把 A 的旧水合
 * 盖到 B 的世界上。卸载时 cancel，迟到回调直接废弃。
 */
export function useChatStoreHydrate(input: {
  queryClient: QueryClient;
  profileId: string;
  sessionId: string | null;
  onSnapshotHydrated?: HydratedSnapshotHandler;
}): void {
  const { queryClient, profileId, sessionId, onSnapshotHydrated } = input;
  const handlerRef = useRef(onSnapshotHydrated);
  handlerRef.current = onSnapshotHydrated;
  const seqRef = useRef(0);

  useEffect(() => {
    const key = profileId.trim();
    if (!key) return;
    const sessionKey = sessionId ?? "";
    const seq = seqRef.current + 1;
    seqRef.current = seq;
    const isFresh = () => seqRef.current === seq;
    let cancelled = false;

    const applySnapshot = (snapshot: ChatRestoreSnapshot) => {
      useChatSnapshotStore.getState().setSnapshot(key, sessionKey, snapshot);
      queryClient.setQueryData(
        chatStoreQueryKeys.snapshot(key, sessionKey),
        snapshot,
      );
      handlerRef.current?.(snapshot);
    };

    void (async () => {
      try {
        await migrateLegacyChatStoreOnce();
      } catch (error) {
        console.error("chat store migration failed", errorMessage(error));
      }
      if (cancelled || !isFresh()) return;
      try {
        const hint = await fetchHintFromStore(key);
        if (cancelled || !isFresh()) return;
        if (hint) {
          queryClient.setQueryData(chatStoreQueryKeys.hint(key), hint);
          const current = useAcpSessionStore.getState().savedSessionFor(key);
          if (current?.sessionId !== hint.sessionId) {
            useAcpSessionStore.getState().setSavedSessionFor(key, hint);
          }
        }
      } catch (error) {
        console.error("chat hint hydrate failed", errorMessage(error));
      }
      if (cancelled || !isFresh()) return;
      let fromDb: ChatRestoreSnapshot | null = null;
      try {
        fromDb = await fetchSnapshotFromStore(key, sessionKey);
      } catch (error) {
        console.error("chat snapshot hydrate failed", errorMessage(error));
      }
      if (cancelled || !isFresh()) return;
      if (fromDb) {
        applySnapshot(fromDb);
        return;
      }
      // DB miss：未迁移时才回退读一次备层（内存优先，其次旧备层），
      // 命中即进内存镜像 + 回填 DB 收敛，下次同会话直接 DB 命中；
      // 已迁移后备层不再可读，直接空（无僵尸恢复）。
      const fallback = readChatRestore(key, sessionKey);
      if (!fallback) return;
      if (cancelled || !isFresh()) return;
      applySnapshot(fallback);
      backfillSnapshotToStore(key, sessionKey, fallback);
    })();

    return () => {
      cancelled = true;
    };
  }, [queryClient, profileId, sessionId]);
}
