import * as React from "react";
import * as SliderPrimitive from "@radix-ui/react-slider";

import { cn } from "@/lib/utils";

function Slider({
  className,
  orientation = "horizontal",
  ...props
}: React.ComponentProps<typeof SliderPrimitive.Root>) {
  return (
    <SliderPrimitive.Root
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
}

export { Slider };
