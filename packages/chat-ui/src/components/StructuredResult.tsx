import type { ReactNode } from "react";

import { cn } from "@lumina/ui";

import type {
  AssistantAction,
  StructuredResultBlock,
} from "../assistantBlocks";
import { ActionChip } from "./ActionChip";
import { SpoilerGuard } from "./SpoilerGuard";

type Props = {
  block: StructuredResultBlock;
  onAction?: (action: AssistantAction) => void;
  density?: "default" | "compact";
  /** Injected markdown renderer (with citation linkify); falls back to plain text. */
  renderMarkdown?: (markdown: string) => ReactNode;
};

/**
 * Render a structured contract as a document inside the conversation.
 * This is deliberately not a card: sections remain part of the assistant's
 * answer and can contain metadata, prose, lists, citations and actions.
 */
export function StructuredResult({
  block,
  onAction,
  density = "default",
  renderMarkdown,
}: Props) {
  const compact = density === "compact";

  return (
    <article
      className={cn(
        "space-y-3 text-foreground",
        compact ? "text-xs" : "text-sm",
      )}
      data-block-kind={block.kind}
    >
      <header className="space-y-1">
        <h3 className="font-semibold leading-tight">{block.title}</h3>
        {block.contract ? (
          <p className="text-[10px] text-muted-foreground">
            结构化结果
          </p>
        ) : null}
      </header>

      {block.metadata.length > 0 ? (
        <dl className="grid grid-cols-1 gap-x-4 gap-y-1 rounded-md border border-border/70 bg-background/40 px-3 py-2 sm:grid-cols-2">
          {block.metadata.map((item) => (
            <div key={item.id} className="min-w-0">
              <dt className="inline text-muted-foreground">{item.label}：</dt>
              <dd className="inline break-words text-foreground">{item.value}</dd>
            </div>
          ))}
        </dl>
      ) : null}

      <SpoilerGuard level={block.spoilerLevel}>
        {block.summary ? (
          renderMarkdown ? (
            <div className="whitespace-pre-wrap break-words leading-relaxed">
              {renderMarkdown(block.summary)}
            </div>
          ) : (
            <p className="whitespace-pre-wrap break-words leading-relaxed">
              {block.summary}
            </p>
          )
        ) : null}

        {block.sections.map((section) => (
          <section key={section.id} className="space-y-1.5">
            <h4 className="font-medium text-foreground">{section.title}</h4>
            {section.paragraphs.map((paragraph, index) => (
              <div
                key={`${section.id}-paragraph-${index}`}
                className="whitespace-pre-wrap break-words leading-relaxed text-foreground/90"
              >
                {renderMarkdown ? renderMarkdown(paragraph) : paragraph}
              </div>
            ))}
            {section.items.length > 0 ? (
              <ul className="list-disc space-y-1 pl-5 leading-relaxed text-foreground/90">
                {section.items.map((item, index) => (
                  <li key={`${section.id}-item-${index}`}>
                    {renderMarkdown ? renderMarkdown(item) : item}
                  </li>
                ))}
              </ul>
            ) : null}
          </section>
        ))}

        {block.actions.length > 0 ? (
          <div className="flex flex-wrap gap-2 pt-1">
            {block.actions.map((action) => (
              <ActionChip key={action.id} chip={action} onAction={onAction} />
            ))}
          </div>
        ) : null}
      </SpoilerGuard>
    </article>
  );
}
