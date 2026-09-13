import { cn } from "@/lib/utils";

type Props = {
  label?: string;
  className?: string;
};

/** Bouncing dots while Agent has not started text output yet. */
export function ChatWaitingDots({ label, className }: Props) {
  return (
    <div
      className={cn("flex items-center gap-2.5 py-0.5", className)}
      aria-live="polite"
      aria-label={label ?? "等待 Agent 回复"}
    >
      <span className="flex items-center gap-1" aria-hidden>
        {[0, 1, 2].map((index) => (
          <span
            key={index}
            className="chat-wait-dot"
            style={{ animationDelay: `${index * 0.14}s` }}
          />
        ))}
      </span>
      {label ? (
        <span className="text-xs text-muted-foreground">{label}</span>
      ) : null}
    </div>
  );
}
