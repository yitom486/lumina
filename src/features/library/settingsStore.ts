import { create } from "zustand";
import { persist } from "zustand/middleware";

type LibrarySettingsState = {
  roots: string[];
  pollIntervalSecs: number;
  privacyAcknowledged: boolean;
  resolverProvider: "acpAgent" | "directApi";
  agentProfileId: string;
  agentModelId: string;
  agentReasoningEffort: string;
  modelBaseUrl: string;
  modelId: string;
  tmdbLanguage: string;
  patchSettings: (patch: Partial<LibrarySettingsState>) => void;
};

const DEFAULTS = {
  roots: [] as string[],
  pollIntervalSecs: 30,
  privacyAcknowledged: false,
  resolverProvider: "directApi" as const,
  agentProfileId: "",
  agentModelId: "",
  agentReasoningEffort: "",
  modelBaseUrl: "",
  modelId: "",
  tmdbLanguage: "zh-CN",
};

/** Persist only non-secret media-library preferences in WebView storage. */
export const useLibrarySettingsStore = create<LibrarySettingsState>()(
  persist(
    (set) => ({
      ...DEFAULTS,
      patchSettings: (patch) => set((state) => ({ ...state, ...patch })),
    }),
    {
      name: "lumina-library-settings",
      partialize: (state) => ({
        roots: state.roots,
        pollIntervalSecs: state.pollIntervalSecs,
        privacyAcknowledged: state.privacyAcknowledged,
        resolverProvider: state.resolverProvider,
        agentProfileId: state.agentProfileId,
        agentModelId: state.agentModelId,
        agentReasoningEffort: state.agentReasoningEffort,
        modelBaseUrl: state.modelBaseUrl,
        modelId: state.modelId,
        tmdbLanguage: state.tmdbLanguage,
      }),
    },
  ),
);
