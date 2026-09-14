import { Button } from "@lumina/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "@lumina/ui/dropdown-menu";

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
      <DropdownMenuContent align="start" side="bottom" className="z-[100]">
        <DropdownMenuLabel>播放速度</DropdownMenuLabel>
        <DropdownMenuRadioGroup
          value={String(rate)}
          onValueChange={(value) => void setRate(Number(value))}
        >
          {RATES.map((value) => (
            <DropdownMenuRadioItem
              key={value}
              value={String(value)}
              className="pl-10"
            >
              {value}x
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
