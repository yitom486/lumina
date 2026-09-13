import { useQuery, useQueryClient } from "@tanstack/react-query";

import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";

import { listPlaybackFormats } from "../api";
import { usePlayerStore } from "../store";

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
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          type="button"
          variant="outline"
          size="sm"
          className="min-w-16 tabular-nums"
          disabled={busy}
        >
          {current?.label ?? "清晰度"}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" side="bottom">
        <DropdownMenuLabel>清晰度</DropdownMenuLabel>
        {data.formats.map((format) => (
          <DropdownMenuCheckboxItem
            key={format.formatId}
            checked={format.formatId === (playbackFormatId ?? data.currentFormatId)}
            onCheckedChange={() => {
              void (async () => {
                await setPlaybackFormat(format.formatId);
                await queryClient.invalidateQueries({
                  queryKey: ["playback-formats"],
                });
              })();
            }}
          >
            {format.label}
          </DropdownMenuCheckboxItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
