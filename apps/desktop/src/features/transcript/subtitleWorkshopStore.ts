import { create } from "zustand";
import { persist } from "zustand/middleware";

/** Subtitle workshop LLM prefs — separate from main chat Agent settings. */
export type SubtitleWorkshopSettings = {
  /** Harness profile for isolated translate jobs (default codex). */
  profileId: string;
  modelId: string;
  reasoningEffort: string;
};

const DEFAULT: SubtitleWorkshopSettings = {
  profileId: "codex",
  modelId: "",
  reasoningEffort: "",
};

type SubtitleWorkshopStore = SubtitleWorkshopSettings & {
  patchSettings: (patch: Partial<SubtitleWorkshopSettings>) => void;
};

export const useSubtitleWorkshopStore = create<SubtitleWorkshopStore>()(
  persist(
    (set) => ({
      ...DEFAULT,
      patchSettings: (patch) => set((state) => ({ ...state, ...patch })),
    }),
    {
      name: "lumina-subtitle-workshop",
      partialize: (state) => ({
        profileId: state.profileId,
        modelId: state.modelId,
        reasoningEffort: state.reasoningEffort,
      }),
      merge: (persisted, current) => {
        const saved = (persisted ?? {}) as Partial<SubtitleWorkshopSettings>;
        return {
          ...current,
          profileId: saved.profileId ?? current.profileId ?? DEFAULT.profileId,
          modelId: saved.modelId ?? current.modelId ?? DEFAULT.modelId,
          reasoningEffort:
            saved.reasoningEffort ??
            current.reasoningEffort ??
            DEFAULT.reasoningEffort,
        };
      },
    },
  ),
);
