import { useQuery } from "@tanstack/react-query";
import { useEffect } from "react";

import { discoverAcpModels } from "@/features/acp/api";
import {
  defaultAgentProfiles,
  profilesHintFromStore,
} from "@lumina/chat-ui/defaultAgentProfiles";
import {
  buildModelOptions,
  buildReasoningOptions,
  mergeModelDefaults,
  selectedOptionLabel,
} from "@lumina/chat-ui/modelConfig";

import { useSubtitleWorkshopStore } from "@lumina/transcript-ui";

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
    const patch = mergeModelDefaults(
      { modelId, reasoningEffort },
      discoveryQuery.data.options,
    );
    if (Object.keys(patch).length > 0) {
      patchSettings(patch);
    }
  }, [discoveryQuery.data, modelId, reasoningEffort, patchSettings]);

  const modelOptions = buildModelOptions(
    discoveryQuery.data?.options ?? null,
    modelId,
  );
  const reasoningOptions = buildReasoningOptions(
    discoveryQuery.data?.options ?? null,
    reasoningEffort,
  );

  const modelLabel = selectedOptionLabel(modelOptions, modelId, "默认");

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
