import { ArrowRight, BookmarkPlus, MessageCircleQuestion, Play } from "lucide-react";

import { Button } from "@lumina/ui";

import type { AssistantAction, AssistantActionChip } from "../assistantBlocks";

type Props = {
  chip: AssistantActionChip;
  onAction?: (action: AssistantAction) => void;
};

export function ActionChip({ chip, onAction }: Props) {
  const disabled = chip.disabled || !onAction;

  return (
    <Button
      type="button"
      size="sm"
      variant="outline"
      className="h-8 max-w-full px-2.5 text-xs"
      disabled={disabled}
      onClick={() => onAction?.(chip.action)}
    >
      <ActionIcon action={chip.action} />
      <span className="truncate">{chip.label}</span>
    </Button>
  );
}

function ActionIcon({ action }: { action: AssistantAction }) {
  switch (action.type) {
    case "seek":
      return <Play aria-hidden />;
    case "save-note":
      return <BookmarkPlus aria-hidden />;
    case "ask":
      return <MessageCircleQuestion aria-hidden />;
    default:
      return <ArrowRight aria-hidden />;
  }
}
