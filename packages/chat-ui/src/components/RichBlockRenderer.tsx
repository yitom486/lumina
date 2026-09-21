import type { ReactNode } from "react";

import { cn } from "@lumina/ui";

import type {
  AssistantAction,
  AssistantBlock,
  AssistantAnchor,
} from "../assistantBlocks";
import { ActionChip } from "./ActionChip";
import { AgentTaskCard } from "./AgentTaskCard";
import { QuestionCard } from "./QuestionCard";
import { WatchFeedCard } from "./WatchFeedCard";

type Props = {
  blocks: readonly AssistantBlock[];
  onAction?: (action: AssistantAction) => void;
  /** Inject the existing ChatMarkdown here; the renderer never parses HTML. */
  renderMarkdown?: (markdown: string) => ReactNode;
  className?: string;
  density?: "default" | "compact";
};

/** Closed-world renderer: input must already be the normalized AssistantBlock union. */
export function RichBlockRenderer({
  blocks,
  onAction,
  renderMarkdown,
  className,
  density = "default",
}: Props) {
  return (
    <div className={cn("space-y-3", className)} data-rich-blocks>
      {blocks.map((block, index) => (
        <BlockView
          key={`${block.id}-${index}`}
          block={block}
          onAction={onAction}
          renderMarkdown={renderMarkdown}
          density={density}
        />
      ))}
    </div>
  );
}

function BlockView({
  block,
  onAction,
  renderMarkdown,
  density,
}: {
  block: AssistantBlock;
  onAction?: (action: AssistantAction) => void;
  renderMarkdown?: (markdown: string) => ReactNode;
  density: "default" | "compact";
}) {
  switch (block.kind) {
    case "narrative":
      return (
        <section className="text-sm leading-relaxed text-foreground" data-block-kind={block.kind}>
          {renderMarkdown ? (
            renderMarkdown(block.markdown)
          ) : (
            <p className="whitespace-pre-wrap break-words">{block.markdown}</p>
          )}
        </section>
      );
    case "transcript-quote":
      return <TranscriptQuote block={block} />;
    case "timeline":
      return (
        <section
          className="space-y-2 rounded-lg border border-border bg-muted/20 p-3"
          data-block-kind={block.kind}
        >
          <h3 className="font-medium text-foreground">{block.title}</h3>
          <ol className="space-y-2 border-l border-border pl-3">
            {block.items.map((item) => (
              <li key={item.id} className="relative space-y-0.5 pl-2">
                <span
                  className="absolute -left-[1.05rem] top-1.5 size-2 rounded-full bg-primary"
                  aria-hidden
                />
                <p className="text-xs font-medium text-muted-foreground">
                  {formatTime(item.atMs)}
                </p>
                <p className="text-sm font-medium text-foreground">{item.title}</p>
                {item.summary ? (
                  <p className="whitespace-pre-wrap break-words text-xs leading-relaxed text-muted-foreground">
                    {item.summary}
                  </p>
                ) : null}
              </li>
            ))}
          </ol>
        </section>
      );
    case "watch-feed-card":
      return <WatchFeedCard block={block} onAction={onAction} density={density} />;
    case "question-card":
      return <QuestionCard block={block} onAction={onAction} density={density} />;
    case "agent-task-status":
      return <AgentTaskCard block={block} />;
    case "action-chip":
      return (
        <div className="flex flex-wrap" data-block-kind={block.kind}>
          <ActionChip chip={block} onAction={onAction} />
        </div>
      );
  }
}

function TranscriptQuote({
  block,
}: {
  block: Extract<AssistantBlock, { kind: "transcript-quote" }>;
}) {
  return (
    <figure
      className="space-y-2 rounded-lg border-l-2 border-primary bg-muted/20 px-3 py-2"
      data-block-kind={block.kind}
    >
      <blockquote className="whitespace-pre-wrap break-words text-sm leading-relaxed text-foreground">
        {block.quote}
      </blockquote>
      <figcaption className="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-muted-foreground">
        {block.speaker ? <span>{block.speaker}</span> : null}
        <span>来源：{formatAnchor(block.anchor)}</span>
      </figcaption>
    </figure>
  );
}

function formatAnchor(anchor: AssistantAnchor): string {
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
