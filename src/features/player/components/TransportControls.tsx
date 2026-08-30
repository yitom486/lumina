import { Button } from "@/components/ui/button";

import { usePlayerStore } from "../store";

export function TransportControls() {
  const busy = usePlayerStore((s) => s.busy);
  const openFile = usePlayerStore((s) => s.openFile);
  const play = usePlayerStore((s) => s.play);
  const pause = usePlayerStore((s) => s.pause);
  const stop = usePlayerStore((s) => s.stop);

  return (
    <div className="flex flex-wrap items-center gap-3">
      <Button type="button" onClick={() => void openFile()} disabled={busy}>
        Open
      </Button>
      <Button
        type="button"
        variant="outline"
        onClick={() => void play()}
        disabled={busy}
      >
        Play
      </Button>
      <Button
        type="button"
        variant="outline"
        onClick={() => void pause()}
        disabled={busy}
      >
        Pause
      </Button>
      <Button
        type="button"
        variant="outline"
        onClick={() => void stop()}
        disabled={busy}
      >
        Stop
      </Button>
    </div>
  );
}
