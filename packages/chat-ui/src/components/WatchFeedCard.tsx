import { Bookmark, Clock3 } from "lucide-react";

import { cn } from "@lumina/ui";

import type {
  AssistantAction,
  WatchFeedCardBlock,
} from "../assistantBlocks";
import { ActionChip } from "./ActionChip";
import { SpoilerGuard } from "./SpoilerGuard";

type Props = {
  block: WatchFeedCardBlock;
  onAction?: (action: AssistantAction) => void;
};

export function WatchFeedCard({ block, onAction }: Props) {
  return (
    <article
      className="space-y-3 rounded-lg border border-border bg-background p-3 shadow-sm"
      data-block-kind={block.kind}
    >
      <header className="space-y-1">
        {block.eyebrow ? (
          <p className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
            {block.eyebrow}
          </p>
        ) : null}
        <h3 className="text-sm font-semibold text-foreground">{block.title}</h3>
      </header>

      <SpoilerGuard level={block.spoilerLevel}>
        {block.summary ? (
          <p className="whitespace-pre-wrap break-words text-sm leading-relaxed text-foreground/90">
            {block.summary}
          </p>
        ) : null}
        {block.bullets.length > 0 ? (
          <ul className="list-disc space-y-1 pl-5 text-sm leading-relaxed text-foreground/90">
            {block.bullets.map((bullet, index) => (
              <li key={`${block.id}-bullet-${index}`}>{bullet}</li>
            ))}
          </ul>
        ) : null}
        {block.anchor ? (
          <p className="flex items-center gap-1.5 text-xs text-muted-foreground">
            <Clock3 className="size-3.5" aria-hidden />
            来源：{formatAnchor(block.anchor)}
          </p>
        ) : null}
        {block.actions.length > 0 ? (
          <div className="flex flex-wrap gap-2 pt-1">
            {block.actions.map((action) => (
              <ActionChip key={action.id} chip={action} onAction={onAction} />
            ))}
          </div>
        ) : null}
      </SpoilerGuard>

      {block.spoilerLevel === "current" ? (
        <p className={cn("flex items-center gap-1.5 text-[11px]", "text-muted-foreground")}>
          <Bookmark className="size-3.5" aria-hidden />
          内容基于当前观看位置
        </p>
      ) : null}
    </article>
  );
}

function formatAnchor(anchor: NonNullable<WatchFeedCardBlock["anchor"]>): string {
  if (typeof anchor.startMs === "number") return formatTime(anchor.startMs);
  return `章节 ${anchor.chapterId}`;
}

function formatTime(milliseconds: number): string {
  if (!Number.isFinite(milliseconds) || milliseconds < 0) return "时间轴";
  const totalSeconds = Math.floor(milliseconds / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}:${seconds.toString().padStart(2, "0")}`;
}
