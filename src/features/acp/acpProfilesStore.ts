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
  return persisted.map((item) =>
    normalizeProfileInput(item as AgentProfileInput),
  );
}

type AcpProfilesStore = {
  activeProfileId: string;
  profiles: AgentProfileInput[];
  setActiveProfileId: (id: string) => void;
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
        const saved = (persisted ?? {}) as Partial<AcpProfilesStore>;
        return {
          ...current,
          ...saved,
          activeProfileId: saved.activeProfileId || current.activeProfileId,
          profiles: mergeProfiles(saved.profiles, current.profiles),
        };
      },
    },
  ),
);
