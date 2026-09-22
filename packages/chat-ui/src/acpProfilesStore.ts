import { create } from "zustand";
import { persist } from "zustand/middleware";

import {
  defaultAgentProfiles,
  normalizeProfileInput,
  profilesHintFromStore,
} from "./defaultAgentProfiles";
import type { AgentProfileInput, AgentProfilesHint } from "./types";

function mergeProfiles(
  persisted: unknown,
  fallback: AgentProfileInput[],
): AgentProfileInput[] {
  if (!Array.isArray(persisted) || persisted.length === 0) {
    return fallback;
  }
  const persistedList = persisted.map((item) =>
    normalizeProfileInput(item as AgentProfileInput),
  );
  for (const fallbackItem of fallback) {
    if (!persistedList.some((item) => item.id === fallbackItem.id)) {
      persistedList.push(fallbackItem);
    }
  }
  return persistedList;
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
        return {
          ...current,
          activeProfileId: saved.activeProfileId ?? current.activeProfileId,
          profiles: mergeProfiles(saved.profiles, current.profiles),
        };
      },
    },
  ),
);
