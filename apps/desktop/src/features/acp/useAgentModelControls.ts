import { useEffect, useMemo, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";

import { errorMessage } from "@/lib/format";

import { useAcpProfilesStore } from "./acpProfilesStore";
import { useAcpSettingsStore } from "./acpSettingsStore";
import { acpSetSessionModel, discoverAcpModels } from "./api";
import { profilesHintFromStore } from "./defaultAgentProfiles";
import {
  buildModelOptions,
  buildReasoningOptions,
  mergeSessionModelDefaults,
} from "./modelOptions";
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
  const [discoveredOptions, setDiscoveredOptions] =
    useState<AcpSessionModelOptions | null>(null);
  const [discoverBusy, setDiscoverBusy] = useState(false);
  const [controlError, setControlError] = useState<string | null>(null);

  const activeProfileId = useAcpProfilesStore((s) => s.activeProfileId);
  const profiles = useAcpProfilesStore((s) => s.profiles);
  const permissionMode = useAcpSettingsStore((s) => s.permissionMode);
  const modelId = useAcpSettingsStore((s) => s.modelId ?? "");
  const reasoningEffort = useAcpSettingsStore((s) => s.reasoningEffort ?? "");
  const patchSettings = useAcpSettingsStore((s) => s.patchSettings);

  const optionSource =
    status?.sessionModelOptions ?? discoveredOptions ?? null;

  useEffect(() => {
    if (!optionSource) return;
    const saved = useAcpSettingsStore.getState();
    const patch = mergeSessionModelDefaults(
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

  const selectedModelLabel =
    modelOptions.find((option) => option.value === modelId)?.name ??
    (modelId.trim() ? modelId : "Agent 默认");

  const selectedReasoningLabel =
    reasoningOptions.find((option) => option.value === reasoningEffort)?.name ??
    (reasoningEffort.trim() ? reasoningEffort : "默认");

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
        setDiscoveredOptions(result.options);
        const patch = mergeSessionModelDefaults(
          { modelId, reasoningEffort },
          result.options,
        );
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
