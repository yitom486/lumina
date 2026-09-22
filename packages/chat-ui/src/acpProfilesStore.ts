import { create } from "zustand";
import { persist } from "zustand/middleware";

import {
  defaultAgentProfiles,
  isDefaultCodexPackageSpec,
  normalizeProfileInput,
  profilesHintFromStore,
} from "./defaultAgentProfiles";
import type { AgentProfileInput, AgentProfilesHint } from "./types";

/**
 * Profile ids removed from the app. Copies persisted by older builds must be
 * dropped on load: the backend no longer deserializes their kind/preset
 * variants, and a single stale entry used to poison the whole profiles hint
 * (every ACP command failed argument parsing → "未配置" + generic retry).
 */
const REMOVED_PROFILE_IDS = new Set(["antigravity"]);

/** Kind/launcher/env/auth variant markers of removed runtimes. Matched by id
 * above first; these catch hand-renamed copies carrying the stale variants. */
const REMOVED_VARIANT_MARKERS = new Set([
  "Antigravity",
  "antigravity-acp",
  "antigravity-proxy",
  "antigravity-oauth",
]);

function isRemovedProfile(item: AgentProfileInput): boolean {
  if (REMOVED_PROFILE_IDS.has(item.id)) return true;
  const markers = [item.kind, item.launcher, item.envPreset, item.authPolicy];
  return markers.some(
    (marker) =>
      typeof marker === "string" && REMOVED_VARIANT_MARKERS.has(marker),
  );
}

/**
 * Reconcile persisted profiles with the current builtin lineup. Runs on every
 * rehydration so upgrades never leave stale entries behind:
 * - drop removed runtimes (by id or by stale kind/preset markers);
 * - refresh builtin shells (display name / presets) from the new defaults
 *   while preserving the user's own launch command/args/env (except codex:
 *   untouched same-package specs of any version upgrade to the floating
 *   default, so nobody stays stuck on a stale cached adapter);
 * - emit builtins in default order, user custom ids after, extras last.
 */
export function mergeProfiles(
  persisted: unknown,
  fallback: AgentProfileInput[],
): AgentProfileInput[] {
  const saved = Array.isArray(persisted) ? persisted : [];
  const byId = new Map<string, AgentProfileInput>();
  for (const raw of saved) {
    const item = normalizeProfileInput(raw as AgentProfileInput);
    if (!item.id || isRemovedProfile(item)) continue;
    byId.set(item.id, item);
  }
  const next: AgentProfileInput[] = [];
  for (const fallbackItem of fallback) {
    const kept = byId.get(fallbackItem.id);
    byId.delete(fallbackItem.id);
    if (!kept) {
      next.push(fallbackItem);
      continue;
    }
    next.push({
      ...fallbackItem,
      command: kept.command ?? fallbackItem.command,
      // codex 例外：裸包名只按天重新解析，旧版 shape 指向哪个缓存版本全凭
      // 运气（曾解析到 1.7.0 的 5.6 时代目录）。同包任意版本都算"原封未动"，
      // 一律升级到当前默认；用户自己改过的 args 一律保留。
      args:
        fallbackItem.id === "codex" &&
        kept.args?.length === 1 &&
        isDefaultCodexPackageSpec(kept.args[0])
          ? (fallbackItem.args ?? kept.args)
          : (kept.args ?? fallbackItem.args),
      env: kept.env ?? fallbackItem.env,
    });
  }
  for (const extra of byId.values()) next.push(extra);
  return next.length > 0 ? next : fallback;
}

type AcpProfilesStore = {
  activeProfileId: string;
  profiles: AgentProfileInput[];
  /** Bare setter: only flips the key. Prefer switchActiveProfileId for UI switches. */
  setActiveProfileId: (id: string) => void;
  /**
   * 切换世界 + 清瞬态的唯一入口（对标隔壁 setSelectedRuntimeId）。
   * 本 store 只做换键：同 id 与空白 id 直接 no-op；瞬态清理
   * （promptQueue/drainLock、pendingPermission、progress + seal、
   * proposal 记账、attachments、模型选择）由调用方 AcpPanel
   * handleSwitchProfile 执行。savedSessions hint 与 chatRestore
   * 快照按 profile 分键保留，这里绝不删任何键。
   */
  switchActiveProfileId: (id: string) => void;
  upsertProfile: (profile: AgentProfileInput) => void;
  profilesHint: () => AgentProfilesHint;
};

/** Agent spawn profiles; persisted in WebView storage, passed to Rust on invoke. */
export const useAcpProfilesStore = create<AcpProfilesStore>()(
  persist(
    (set, get) => ({
      activeProfileId: "codex",
      profiles: defaultAgentProfiles(),
      setActiveProfileId: (activeProfileId) => set({ activeProfileId }),
      switchActiveProfileId: (id) => {
        const next = id.trim();
        if (!next) return;
        set((state) =>
          state.activeProfileId === next ? state : { activeProfileId: next },
        );
      },
      upsertProfile: (profile) => {
        const next = normalizeProfileInput(profile);
        set((state) => {
          const profiles = [...state.profiles];
          const index = profiles.findIndex((item) => item.id === next.id);
          if (index >= 0) {
            profiles[index] = next;
          } else {
            profiles.push(next);
          }
          return { profiles };
        });
      },
      profilesHint: () =>
        profilesHintFromStore(get().activeProfileId, get().profiles),
    }),
    {
      name: "lumina-acp-profiles",
      partialize: (state) => ({
        activeProfileId: state.activeProfileId,
        profiles: state.profiles,
      }),
      merge: (persisted, current) => {
        const saved = (persisted ?? {}) as Partial<
          Pick<AcpProfilesStore, "activeProfileId" | "profiles">
        >;
        const profiles = mergeProfiles(saved.profiles, current.profiles);
        const active = saved.activeProfileId ?? current.activeProfileId;
        return {
          ...current,
          activeProfileId: profiles.some((item) => item.id === active)
            ? active
            : current.activeProfileId,
          profiles,
        };
      },
    },
  ),
);
