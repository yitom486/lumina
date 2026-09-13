import { History, Trash2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

import {
  formatConversationTime,
  type SavedChatConversation,
} from "../chatHistoryStore";

type Props = {
  open: boolean;
  items: SavedChatConversation[];
  activeId: string | null;
  scopeLabel: string;
  includeAll: boolean;
  onToggleScope: () => void;
  onClose: () => void;
  onSelect: (id: string) => void;
  onDelete: (id: string) => void;
};

/** Inline panel below the toolbar — avoids absolute overlay clipping in ChatDock. */
export function ChatHistorySheet({
  open,
  items,
  activeId,
  scopeLabel,
  includeAll,
  onToggleScope,
  onClose,
  onSelect,
  onDelete,
}: Props) {
  if (!open) return null;

  return (
    <div className="border-b border-border bg-popover shadow-sm">
      <div className="flex items-center justify-between gap-2 px-1 py-1.5">
        <div className="inline-flex min-w-0 items-center gap-1.5 text-[11px] font-medium">
          <History className="size-3.5 shrink-0" />
          <span className="truncate">历史对话</span>
        </div>
        <div className="flex shrink-0 items-center gap-1">
          <Button
            size="sm"
            variant="ghost"
            className="h-7 max-w-[7rem] truncate px-2 text-[10px]"
            onClick={onToggleScope}
          >
            {includeAll ? "仅当前视频" : scopeLabel}
          </Button>
          <Button
            size="sm"
            variant="ghost"
            className="h-7 px-2 text-[10px]"
            onClick={onClose}
          >
            关闭
          </Button>
        </div>
      </div>

      {items.length === 0 ? (
        <p className="px-3 pb-4 pt-1 text-center text-[11px] text-muted-foreground">
          暂无已保存的对话记录
        </p>
      ) : (
        <ul className="max-h-56 overflow-y-auto pb-1">
          {items.map((item) => (
            <li key={item.id}>
              <div
                className={cn(
                  "flex items-start gap-2 px-2 py-2 hover:bg-muted/60",
                  item.id === activeId && "bg-muted/40",
                )}
              >
                <button
                  type="button"
                  className="min-w-0 flex-1 text-left"
                  onClick={() => onSelect(item.id)}
                >
                  <p className="truncate text-[11px] font-medium text-foreground">
                    {item.title}
                  </p>
                  <p className="mt-0.5 text-[10px] text-muted-foreground">
                    {formatConversationTime(item.updatedAtMs)}
                    {item.turns.length > 0
                      ? ` · ${item.turns.length} 轮`
                      : null}
                  </p>
                </button>
                <Button
                  size="icon"
                  variant="ghost"
                  className="size-7 shrink-0 text-muted-foreground hover:text-destructive"
                  aria-label="删除对话"
                  onClick={() => onDelete(item.id)}
                >
                  <Trash2 className="size-3.5" />
                </Button>
              </div>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
