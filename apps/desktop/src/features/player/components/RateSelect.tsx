import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuCheckboxItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";

import { usePlayerStore } from "../store";

const RATES = [0.5, 0.75, 1, 1.25, 1.5, 2];

export function RateSelect() {
  const rate = usePlayerStore((s) => s.rate);
  const setRate = usePlayerStore((s) => s.setRate);

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          type="button"
          variant="outline"
          size="sm"
          className="min-w-14 tabular-nums"
        >
          {rate}x
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" side="bottom">
        <DropdownMenuLabel>播放速度</DropdownMenuLabel>
        {RATES.map((value) => (
          <DropdownMenuCheckboxItem
            key={value}
            checked={rate === value}
            onCheckedChange={() => void setRate(value)}
          >
            {value}x
          </DropdownMenuCheckboxItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
