import { usePlayerStore } from "../store";

const RATES = [0.5, 0.75, 1, 1.25, 1.5, 2];

export function RateSelect() {
  const rate = usePlayerStore((s) => s.rate);
  const setRate = usePlayerStore((s) => s.setRate);

  return (
    <label className="flex items-center gap-2 text-sm">
      Rate
      <select
        className="rounded border border-border bg-background px-2 py-1"
        value={rate}
        onChange={(e) => void setRate(Number(e.target.value))}
        aria-label="Playback rate"
      >
        {RATES.map((value) => (
          <option key={value} value={value}>
            {value}x
          </option>
        ))}
      </select>
    </label>
  );
}
