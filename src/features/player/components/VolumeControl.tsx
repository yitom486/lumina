import { usePlayerStore } from "../store";

export function VolumeControl() {
  const volume = usePlayerStore((s) => s.volume);
  const setVolume = usePlayerStore((s) => s.setVolume);

  return (
    <label className="flex items-center gap-2 text-sm">
      Volume
      <input
        type="range"
        min={0}
        max={100}
        step={1}
        value={volume}
        onChange={(e) => void setVolume(Number(e.target.value))}
        aria-label="Volume"
      />
      <span className="w-8 tabular-nums text-muted-foreground">
        {Math.round(volume)}
      </span>
    </label>
  );
}
