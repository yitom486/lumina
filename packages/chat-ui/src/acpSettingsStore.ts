import { create } from "zustand";
import { persist } from "zustand/middleware";

import {
  DEFAULT_TRANSCRIPT_WINDOW_PRESET,
  normalizeTranscriptWindowPreset,
  type AcpClientSettings,
  type TranscriptWindowPreset,
} from "./types";

type AcpSettingsStore = AcpClientSettings & {
  transcriptWindowPreset: TranscriptWindowPreset;
  patchSettings: (
    patch: Partial<AcpClientSettings> & {
      transcriptWindowPreset?: TranscriptWindowPreset;
    },
  ) => void;
};

const DEFAULT_SETTINGS: Omit<AcpSettingsStore, "patchSettings"> = {
  permissionMode: "auto",
  thinkingLevel: "minimal",
  agentMode: "default",
  visionCapable: true,
  modelId: "",
  reasoningEffort: "",
  transcriptWindowPreset: DEFAULT_TRANSCRIPT_WINDOW_PRESET,
};

export function mergeAcpSettings(
  persisted: unknown,
  current: AcpSettingsStore,
): AcpSettingsStore {
  const saved = (persisted ?? {}) as Partial<
    AcpClientSettings & { transcriptWindowPreset?: unknown }
  >;
  return {
    ...current,
    permissionMode:
      saved.permissionMode ?? current.permissionMode ?? DEFAULT_SETTINGS.permissionMode,
    thinkingLevel:
      saved.thinkingLevel ?? current.thinkingLevel ?? DEFAULT_SETTINGS.thinkingLevel,
    agentMode: saved.agentMode ?? current.agentMode ?? DEFAULT_SETTINGS.agentMode,
    visionCapable:
      saved.visionCapable ?? current.visionCapable ?? DEFAULT_SETTINGS.visionCapable,
    modelId: saved.modelId ?? current.modelId ?? DEFAULT_SETTINGS.modelId,
    reasoningEffort:
      saved.reasoningEffort ?? current.reasoningEffort ?? DEFAULT_SETTINGS.reasoningEffort,
    transcriptWindowPreset: normalizeTranscriptWindowPreset(
      saved.transcriptWindowPreset ??
        current.transcriptWindowPreset ??
        DEFAULT_TRANSCRIPT_WINDOW_PRESET,
    ),
  };
}

/** Agent client prefs (permission, mode, thinking display); persisted in WebView storage. */
export const useAcpSettingsStore = create<AcpSettingsStore>()(
  persist(
    (set) => ({
      ...DEFAULT_SETTINGS,
      patchSettings: (patch) =>
        set((state) => ({
          ...state,
          ...patch,
          transcriptWindowPreset:
            patch.transcriptWindowPreset === undefined
              ? state.transcriptWindowPreset
              : normalizeTranscriptWindowPreset(patch.transcriptWindowPreset),
        })),
    }),
    {
      name: "lumina-acp-settings",
      partialize: (state) => ({
        permissionMode: state.permissionMode,
        thinkingLevel: state.thinkingLevel,
        agentMode: state.agentMode,
        visionCapable: state.visionCapable,
        modelId: state.modelId,
        reasoningEffort: state.reasoningEffort,
        transcriptWindowPreset: state.transcriptWindowPreset,
      }),
      merge: (persisted, current) => mergeAcpSettings(persisted, current),
    },
  ),
);

export function clientSettingsFromStore(
  settings: AcpClientSettings,
): AcpClientSettings {
  return {
    permissionMode: settings.permissionMode,
    thinkingLevel: settings.thinkingLevel,
    agentMode: settings.agentMode,
    transcriptWindowPreset: normalizeTranscriptWindowPreset(
      settings.transcriptWindowPreset,
    ),
    visionCapable: settings.visionCapable ?? true,
    modelId: settings.modelId?.trim() || null,
    reasoningEffort: settings.reasoningEffort?.trim() || null,
  };
}
