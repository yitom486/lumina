import { useQuery, useQueryClient } from "@tanstack/react-query";

import { listPlaybackFormats } from "../api";
import { usePlayerStore } from "../store";
import { SelectionCombobox } from "./SelectionCombobox";

export function QualitySelect() {
  const sourceKind = usePlayerStore((s) => s.sourceKind);
  const playbackFormatId = usePlayerStore((s) => s.playbackFormatId);
  const busy = usePlayerStore((s) => s.busy);
  const setPlaybackFormat = usePlayerStore((s) => s.setPlaybackFormat);
  const queryClient = useQueryClient();

  const remote = sourceKind === "remote";
  const { data } = useQuery({
    queryKey: ["playback-formats", playbackFormatId],
    queryFn: listPlaybackFormats,
    enabled: remote,
    staleTime: 30_000,
  });

  if (!remote || !data?.formats.length) {
    return null;
  }

  const current =
    data.formats.find((f) => f.formatId === (playbackFormatId ?? data.currentFormatId)) ??
    data.formats[0];

  return (
    <SelectionCombobox
      value={playbackFormatId ?? data.currentFormatId ?? ""}
      options={data.formats.map((format) => ({
        value: format.formatId,
        label: format.label,
      }))}
      placeholder="清晰度"
      ariaLabel="选择清晰度"
      triggerLabel={current?.label ?? "清晰度"}
      disabled={busy}
      className="min-w-16 tabular-nums"
      onValueChange={(formatId) => {
        void (async () => {
          await setPlaybackFormat(formatId);
          await queryClient.invalidateQueries({
            queryKey: ["playback-formats"],
          });
        })();
      }}
    />
  );
}
