import { usePlayerStore } from "../store";
import { SelectionCombobox } from "./SelectionCombobox";

const RATES = [0.5, 0.75, 1, 1.25, 1.5, 2];

export function RateSelect() {
  const rate = usePlayerStore((s) => s.rate);
  const setRate = usePlayerStore((s) => s.setRate);

  return (
    <SelectionCombobox
      value={String(rate)}
      options={RATES.map((value) => ({
        value: String(value),
        label: `${value}x`,
      }))}
      placeholder="倍速"
      ariaLabel="选择播放速度"
      triggerLabel={`${rate}x`}
      className="min-w-14 tabular-nums"
      onValueChange={(value) => void setRate(Number(value))}
    />
  );
}
