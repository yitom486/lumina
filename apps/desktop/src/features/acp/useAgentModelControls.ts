import { useEffect, useMemo, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";

import { errorMessage } from "@/lib/format";

import { useAcpProfilesStore } from "@lumina/chat-ui/acpProfilesStore";
import { useAcpSettingsStore } from "@lumina/chat-ui/acpSettingsStore";
import { acpSetSessionModel, discoverAcpModels } from "./api";
import { profilesHintFromStore } from "@lumina/chat-ui/defaultAgentProfiles";
import {
  buildModelOptions,
  buildReasoningOptions,
  mergeModelDefaults,
  selectedOptionLabel,
} from "@lumina/chat-ui/modelConfig";
import type { AcpSessionModelOptions, AcpStatus } from "./types";

type Options = {
  status: AcpStatus | undefined;
  busy?: boolean;
  sessionConnected?: boolean;
};

export function useAgentModelControls({
  status,
  busy,
  sessionConnected,
}: Options) {
  const queryClient = useQueryClient();
  // 发现结果按 profile 分桶（对标隔壁 modelCatalogByRuntime 与 chatRestore
  // pendingByProfile）：切换只失活旧键（下读新键），不清全量，切回即复用。
  const [discoveredOptionsByProfile, setDiscoveredOptionsByProfile] = useState<
    Record<string, AcpSessionModelOptions>
  >({});
  const [discoverBusy, setDiscoverBusy] = useState(false);
  const [controlError, setControlError] = useState<string | null>(null);

  const activeProfileId = useAcpProfilesStore((s) => s.activeProfileId);
  const profiles = useAcpProfilesStore((s) => s.profiles);
  const permissionMode = useAcpSettingsStore((s) => s.permissionMode);
  const modelId = useAcpSettingsStore((s) => s.modelId ?? "");
  const reasoningEffort = useAcpSettingsStore((s) => s.reasoningEffort ?? "");
  const patchSettings = useAcpSettingsStore((s) => s.patchSettings);

  // 当前世界只读自家桶：旧键躺着不动，新世界读不到旧世界的模型。
  const discoveredOptions = activeProfileId
    ? (discoveredOptionsByProfile[activeProfileId] ?? null)
    : null;
  const optionSource =
    status?.activeProfileId === activeProfileId
      ? (status?.sessionModelOptions ?? discoveredOptions ?? null)
      : (discoveredOptions ?? null);

  // Discovery results describe one agent's session; switching profile must
  // not carry, say, Codex GPT models into Antigravity's composer.
  // 分桶后切换不清全量：只清 transient 的 controlError，各家发现结果保留。
  useEffect(() => {
    setControlError(null);
  }, [activeProfileId]);

  useEffect(() => {
    if (!optionSource) return;
    const saved = useAcpSettingsStore.getState();
    const patch = mergeModelDefaults(
      {
        modelId: saved.modelId ?? "",
        reasoningEffort: saved.reasoningEffort ?? "",
      },
      optionSource,
    );
    if (Object.keys(patch).length > 0) {
      patchSettings(patch);
    }
  }, [optionSource, patchSettings]);

  const modelOptions = useMemo(
    () => buildModelOptions(optionSource, modelId),
    [modelId, optionSource],
  );
  const reasoningOptions = useMemo(
    () => buildReasoningOptions(optionSource, reasoningEffort),
    [optionSource, reasoningEffort],
  );

  const selectedModelLabel = selectedOptionLabel(
    modelOptions,
    modelId,
    "Agent 默认",
  );

  const selectedReasoningLabel = selectedOptionLabel(
    reasoningOptions,
    reasoningEffort,
    "默认",
  );

  const controlsDisabled = Boolean(busy || discoverBusy);

  const applyModelSelection = async (next: {
    modelId?: string;
    reasoningEffort?: string;
  }) => {
    patchSettings(next);
    if (!sessionConnected || busy) return;

    try {
      setControlError(null);
      await acpSetSessionModel({
        modelId: next.modelId ?? modelId,
        reasoningEffort: next.reasoningEffort ?? reasoningEffort,
      });
      await queryClient.invalidateQueries({ queryKey: ["acp-status"] });
    } catch (error) {
      setControlError(errorMessage(error));
    }
  };

  const discoverModels = async () => {
    setDiscoverBusy(true);
    setControlError(null);
    try {
      const result = await discoverAcpModels(
        profilesHintFromStore(activeProfileId, profiles),
        activeProfileId,
      );
      if (result.connected) {
        const owner = activeProfileId;
        setDiscoveredOptionsByProfile((prev) => ({ ...prev, [owner]: result.options }));
        const patch = mergeModelDefaults({ modelId, reasoningEffort }, result.options);
        if (Object.keys(patch).length > 0) {
          patchSettings(patch);
        }
      } else {
        setControlError(result.message);
      }
    } catch (error) {
      setControlError(errorMessage(error));
    } finally {
      setDiscoverBusy(false);
    }
  };

  return {
    permissionMode,
    discoveredOptions,
    discoveredOptionsByProfile,
    modelId,
    reasoningEffort,
    modelOptions,
    reasoningOptions,
    selectedModelLabel,
    selectedReasoningLabel,
    controlsDisabled,
    controlError,
    discoverBusy,
    hasModelOptions: modelOptions.length > 0,
    hasReasoningOptions: reasoningOptions.length > 0,
    patchSettings,
    applyModelSelection,
    discoverModels,
    canDiscover: !sessionConnected && Boolean(status?.available),
  };
}
