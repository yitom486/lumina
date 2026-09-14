/**
 * VENDOR PRISTINE — DO NOT EDIT.
 * shadcn/ui default style: https://ui.shadcn.com/docs/components/separator
 * Customization is forbidden in this file. Adapt at the call site via
 * `className` (merged through cn/tailwind-merge) or in an app-side
 * transferred copy. Mechanical adaptations vs upstream: `@/lib/utils` →
 * `./utils`; `"use client"` omitted (Vite/Tauri, no RSC).
 */
import * as React from "react";
import * as SeparatorPrimitive from "@radix-ui/react-separator";

import { cn } from "./utils";

const Separator = React.forwardRef<
  React.ElementRef<typeof SeparatorPrimitive.Root>,
  React.ComponentPropsWithoutRef<typeof SeparatorPrimitive.Root>
>(
  (
    { className, orientation = "horizontal", decorative = true, ...props },
    ref,
  ) => (
    <SeparatorPrimitive.Root
      ref={ref}
      decorative={decorative}
      orientation={orientation}
      className={cn(
        "shrink-0 bg-border",
        orientation === "horizontal" ? "h-[1px] w-full" : "h-full w-[1px]",
        className,
      )}
      {...props}
    />
  ),
);
Separator.displayName = SeparatorPrimitive.Root.displayName;

export { Separator };
