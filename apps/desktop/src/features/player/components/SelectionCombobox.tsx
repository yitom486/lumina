import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { Check, ChevronsUpDown, Search } from "lucide-react";

import { Button } from "@lumina/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@lumina/ui/dropdown-menu";
import { cn } from "@lumina/ui/utils";

export type SelectionOption = {
  value: string;
  label: string;
  keywords?: string;
};

type ToggleOption = {
  label: string;
  checked: boolean;
  onChange: () => void;
};

type Props = {
  value: string | null | undefined;
  options: SelectionOption[];
  placeholder: string;
  ariaLabel: string;
  onValueChange: (value: string) => void;
  triggerLabel?: string;
  leadingIcon?: ReactNode;
  toggle?: ToggleOption;
  disabled?: boolean;
  className?: string;
  onOpenChange?: (open: boolean) => void;
};

/** App-side shadcn-style combobox built from the existing Radix menu primitive. */
export function SelectionCombobox({
  value,
  options,
  placeholder,
  ariaLabel,
  onValueChange,
  triggerLabel,
  leadingIcon,
  toggle,
  disabled = false,
  className,
  onOpenChange,
}: Props) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);
  const selected = options.find((option) => option.value === value);
  const normalizedQuery = query.trim().toLocaleLowerCase();
  const filteredOptions = useMemo(
    () =>
      normalizedQuery
        ? options.filter((option) =>
            `${option.label} ${option.keywords ?? ""}`
              .toLocaleLowerCase()
              .includes(normalizedQuery),
          )
        : options,
    [normalizedQuery, options],
  );

  useEffect(() => {
    onOpenChange?.(open);
    if (!open) {
      setQuery("");
      return;
    }
    const frame = requestAnimationFrame(() => inputRef.current?.focus());
    return () => cancelAnimationFrame(frame);
  }, [onOpenChange, open]);

  return (
    <DropdownMenu open={open} onOpenChange={setOpen}>
      <DropdownMenuTrigger asChild>
        <Button
          type="button"
          variant="outline"
          size="sm"
          role="combobox"
          aria-expanded={open}
          aria-label={ariaLabel}
          disabled={disabled}
          className={cn("min-w-0 justify-between gap-2", className)}
        >
          {leadingIcon}
          <span className="min-w-0 flex-1 truncate text-left">
            {triggerLabel ?? selected?.label ?? placeholder}
          </span>
          <ChevronsUpDown className="size-3.5 shrink-0 opacity-60" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent
        align="start"
        side="bottom"
        className="z-[100] w-[min(22rem,calc(100vw-2rem))] min-w-[16rem] p-1"
      >
        <div className="flex items-center gap-2 border-b border-border px-2 py-1.5">
          <Search className="size-3.5 shrink-0 text-muted-foreground" />
          <input
            ref={inputRef}
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={(event) => {
              if (event.key !== "Escape") event.stopPropagation();
            }}
            placeholder={`搜索${placeholder}`}
            aria-label={`搜索${placeholder}`}
            className="min-w-0 flex-1 bg-transparent text-sm outline-none placeholder:text-muted-foreground"
          />
        </div>

        {toggle ? (
          <div className="mt-1 flex items-center justify-between gap-3 rounded-md px-2.5 py-2 text-sm">
            <span>{toggle.label}</span>
            <button
              type="button"
              aria-label={toggle.label}
              aria-pressed={toggle.checked}
              onClick={(event) => {
                event.preventDefault();
                event.stopPropagation();
                toggle.onChange();
              }}
              className={cn(
                "relative inline-flex h-5 w-9 shrink-0 rounded-full border border-border transition-colors",
                toggle.checked ? "bg-primary" : "bg-muted",
              )}
            >
              <span
                className={cn(
                  "pointer-events-none size-3.5 self-center rounded-full bg-background shadow transition-transform",
                  toggle.checked ? "translate-x-[17px]" : "translate-x-0.5",
                )}
              />
            </button>
          </div>
        ) : null}

        <div className="mt-1 max-h-56 overflow-y-auto">
          {filteredOptions.length === 0 ? (
            <p className="px-2.5 py-2 text-sm text-muted-foreground">
              没有匹配项
            </p>
          ) : (
            filteredOptions.map((option) => {
              const isSelected = option.value === value;
              return (
                <DropdownMenuItem
                  key={option.value}
                  onSelect={() => {
                    onValueChange(option.value);
                    setOpen(false);
                  }}
                  aria-current={isSelected ? "true" : undefined}
                  className={cn(
                    "cursor-pointer px-2.5 py-2",
                    isSelected && "bg-accent text-accent-foreground",
                  )}
                >
                  <span className="min-w-0 flex-1 truncate">{option.label}</span>
                  {isSelected ? <Check className="size-4 shrink-0" /> : null}
                </DropdownMenuItem>
              );
            })
          )}
        </div>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
