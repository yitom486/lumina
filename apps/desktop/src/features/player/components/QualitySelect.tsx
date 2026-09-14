import { useQuery, useQueryClient } from "@tanstack/react-query";

import { Button } from "@lumina/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "@lumina/ui/dropdown-menu";

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
      <DropdownMenuContent align="start" side="bottom" className="z-[100]">
        <DropdownMenuLabel>清晰度</DropdownMenuLabel>
        <DropdownMenuRadioGroup
          value={playbackFormatId ?? data.currentFormatId ?? ""}
          onValueChange={(formatId) => {
            void (async () => {
              await setPlaybackFormat(formatId);
              await queryClient.invalidateQueries({
                queryKey: ["playback-formats"],
              });
            })();
          }}
        >
          {data.formats.map((format) => (
            <DropdownMenuRadioItem
              key={format.formatId}
              value={format.formatId}
              className="pl-10"
            >
              {format.label}
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
