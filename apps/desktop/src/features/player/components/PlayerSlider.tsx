/**
 * App-side transferred slider: compact player theme + vertical orientation.
 * The vendor original (`@lumina/ui/slider`, shadcn pristine) is horizontal
 * only with a larger thumb — DO NOT re-theme the vendor file; all player
 * slider customization lives here.
 */
import * as React from "react";
import * as SliderPrimitive from "@radix-ui/react-slider";

import { cn } from "@lumina/ui/utils";

const PlayerSlider = React.forwardRef<
  React.ElementRef<typeof SliderPrimitive.Root>,
  React.ComponentPropsWithoutRef<typeof SliderPrimitive.Root> & {
    orientation?: "horizontal" | "vertical";
  }
>(function PlayerSlider(
  { className, orientation = "horizontal", ...props },
  ref,
) {
  return (
    <SliderPrimitive.Root
      ref={ref}
      orientation={orientation}
      className={cn(
        "relative flex touch-none select-none items-center",
        orientation === "horizontal" && "w-full",
        orientation === "vertical" && "h-full w-4 flex-col",
        className,
      )}
      {...props}
    >
      <SliderPrimitive.Track
        className={cn(
          "relative grow overflow-hidden rounded-full bg-muted",
          orientation === "horizontal" && "h-1.5 w-full",
          orientation === "vertical" && "h-full w-1.5",
        )}
      >
        <SliderPrimitive.Range
          className={cn(
            "absolute bg-primary",
            orientation === "horizontal" && "h-full",
            orientation === "vertical" && "w-full",
          )}
        />
      </SliderPrimitive.Track>
      <SliderPrimitive.Thumb className="block size-3.5 rounded-full border border-primary/50 bg-primary shadow transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-50" />
    </SliderPrimitive.Root>
  );
});

PlayerSlider.displayName = "PlayerSlider";

export { PlayerSlider };
