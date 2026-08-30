import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

type Props = {
  value: string;
  disabled?: boolean;
  busy?: boolean;
  placeholder?: string;
  onChange: (value: string) => void;
  onSend: () => void;
  onCancel?: () => void;
};

export function ChatComposer({
  value,
  disabled,
  busy,
  placeholder = "输入问题…",
  onChange,
  onSend,
  onCancel,
}: Props) {
  return (
    <div className="shrink-0 space-y-2 border-t border-border p-3">
      <textarea
        className={cn(
          "min-h-[72px] w-full resize-none rounded-md border border-border bg-background px-2 py-1.5 text-sm",
          "outline-none focus-visible:ring-1 focus-visible:ring-ring",
          "disabled:opacity-60",
        )}
        placeholder={placeholder}
        value={value}
        disabled={disabled || busy}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            if (!disabled && !busy && value.trim()) onSend();
          }
        }}
      />
      <div className="flex gap-2">
        <Button
          size="sm"
          className="flex-1"
          disabled={disabled || busy || !value.trim()}
          onClick={onSend}
        >
          {busy ? "回复中…" : "发送"}
        </Button>
        <Button
          size="sm"
          variant="outline"
          disabled={!busy}
          onClick={() => onCancel?.()}
        >
          取消
        </Button>
      </div>
    </div>
  );
}
