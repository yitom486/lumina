import { useQuery } from "@tanstack/react-query";
import { useEffect } from "react";

import { discoverAcpModels } from "@/features/acp/api";
import {
  defaultAgentProfiles,
  profilesHintFromStore,
} from "@/features/acp/defaultAgentProfiles";
import {
  buildAgentModelOptions,
  buildAgentReasoningOptions,
  mergeAgentDiscoverySettings,
} from "@/features/library/resolverSettings";

import { useSubtitleWorkshopStore } from "./subtitleWorkshopStore";

export function useSubtitleWorkshopModels(enabled = true) {
  const profileId = useSubtitleWorkshopStore((s) => s.profileId);
  const modelId = useSubtitleWorkshopStore((s) => s.modelId);
  const reasoningEffort = useSubtitleWorkshopStore((s) => s.reasoningEffort);
  const patchSettings = useSubtitleWorkshopStore((s) => s.patchSettings);

  const profilesHint = profilesHintFromStore(profileId, defaultAgentProfiles());

  const discoveryQuery = useQuery({
    queryKey: ["subtitleWorkshopModels", profileId],
    queryFn: () => discoverAcpModels(profilesHint, profileId),
    enabled,
    staleTime: 5 * 60_000,
    retry: false,
  });

  useEffect(() => {
    if (!discoveryQuery.data?.connected) return;
    const patch = mergeAgentDiscoverySettings(
      { agentModelId: modelId, agentReasoningEffort: reasoningEffort },
      discoveryQuery.data,
    );
    if (patch.agentModelId || patch.agentReasoningEffort) {
      patchSettings({
        ...(patch.agentModelId ? { modelId: patch.agentModelId } : {}),
        ...(patch.agentReasoningEffort
          ? { reasoningEffort: patch.agentReasoningEffort }
          : {}),
      });
    }
  }, [discoveryQuery.data, modelId, reasoningEffort, patchSettings]);

  const modelOptions = buildAgentModelOptions(discoveryQuery.data ?? null, modelId);
  const reasoningOptions = buildAgentReasoningOptions(
    discoveryQuery.data ?? null,
    reasoningEffort,
  );

  const modelLabel =
    modelOptions.find((option) => option.value === modelId)?.name ??
    (modelId.trim() ? modelId : "默认");

  return {
    profileId,
    modelId,
    reasoningEffort,
    patchSettings,
    modelOptions,
    reasoningOptions,
    modelLabel,
    discoveryQuery,
    profilesHint,
    hasModelOptions: modelOptions.length > 0,
    hasReasoningOptions: reasoningOptions.length > 0,
  };
}
