import { create } from "zustand";
import { persist } from "zustand/middleware";

type LibrarySettingsState = {
  roots: string[];
  pollIntervalSecs: number;
  privacyAcknowledged: boolean;
  modelBaseUrl: string;
  modelId: string;
  modelApiKeyEnv: string;
  tmdbAccessTokenEnv: string;
  tmdbLanguage: string;
  patchSettings: (patch: Partial<LibrarySettingsState>) => void;
};

const DEFAULTS = {
  roots: [] as string[],
  pollIntervalSecs: 30,
  privacyAcknowledged: false,
  modelBaseUrl: "",
  modelId: "",
  modelApiKeyEnv: "LUMINA_METADATA_MODEL_API_KEY",
  tmdbAccessTokenEnv: "LUMINA_TMDB_ACCESS_TOKEN",
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
        modelBaseUrl: state.modelBaseUrl,
        modelId: state.modelId,
        modelApiKeyEnv: state.modelApiKeyEnv,
        tmdbAccessTokenEnv: state.tmdbAccessTokenEnv,
        tmdbLanguage: state.tmdbLanguage,
      }),
    },
  ),
);
