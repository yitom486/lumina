import { create } from "zustand";
import { persist } from "zustand/middleware";

import type { AcpClientSettings } from "./types";

const DEFAULT_SETTINGS: AcpClientSettings = {
  permissionMode: "auto",
  thinkingLevel: "minimal",
  agentMode: "default",
  visionCapable: true,
  modelId: "",
  reasoningEffort: "",
};

type AcpSettingsStore = AcpClientSettings & {
  patchSettings: (patch: Partial<AcpClientSettings>) => void;
};

/** Agent client prefs (permission, mode, thinking display); persisted in WebView storage. */
export const useAcpSettingsStore = create<AcpSettingsStore>()(
  persist(
    (set) => ({
      ...DEFAULT_SETTINGS,
      patchSettings: (patch) => set((state) => ({ ...state, ...patch })),
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
      }),
      merge: (persisted, current) => {
        const saved = (persisted ?? {}) as Partial<AcpClientSettings>;
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
        };
      },
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
    visionCapable: settings.visionCapable ?? true,
    modelId: settings.modelId?.trim() || null,
    reasoningEffort: settings.reasoningEffort?.trim() || null,
  };
}
