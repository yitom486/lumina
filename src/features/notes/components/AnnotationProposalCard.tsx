import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";

import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { errorMessage, formatTime } from "@/lib/format";
import { cn } from "@/lib/utils";

import { createNote, dismissAnnotationProposal } from "../api";
import type { VideoAnnotationProposal } from "../proposalTypes";

type Props = {
  proposal: VideoAnnotationProposal;
  workspace: string;
  className?: string;
  onDismiss: () => void;
  onSaved: () => void;
};

export function AnnotationProposalCard({
  proposal,
  workspace,
  className,
  onDismiss,
  onSaved,
}: Props) {
  const queryClient = useQueryClient();
  const [body, setBody] = useState(proposal.body);
  const [error, setError] = useState<string | null>(null);

  const saveMutation = useMutation({
    mutationFn: async () => {
      const trimmed = body.trim();
      if (!trimmed) {
        throw new Error("批注内容不能为空");
      }
      await createNote({
        mediaPath: proposal.mediaPath,
        positionMs: proposal.positionMs,
        body: trimmed,
        subtitleChoiceId: proposal.subtitleChoiceId,
        anchorCueIndex: proposal.anchorCueIndex,
        quoteCueIndices: proposal.quoteCueIndices,
        quoteHint: proposal.quoteHint,
        includeQuotes: proposal.includeQuotes,
      });
      await dismissAnnotationProposal(workspace);
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({
        queryKey: ["notes", proposal.mediaPath],
      });
      onSaved();
    },
    onError: (cause) => {
      setError(errorMessage(cause));
    },
  });

  const dismissMutation = useMutation({
    mutationFn: () => dismissAnnotationProposal(workspace),
    onSuccess: onDismiss,
    onError: (cause) => {
      setError(errorMessage(cause));
    },
  });

  return (
    <div
      className={cn(
        "rounded-lg border border-primary/30 bg-primary/5 p-3 shadow-sm",
        className,
      )}
    >
      <div className="mb-2 flex items-start justify-between gap-2">
        <div>
          <p className="text-sm font-medium text-foreground">Agent 提议批注</p>
          <p className="text-[11px] text-muted-foreground">
            锚点 {formatTime(proposal.positionMs)} · 确认后才会写入笔记库
          </p>
        </div>
      </div>

      <textarea
        className="mb-2 min-h-[88px] w-full resize-y rounded-md border border-input bg-background px-2 py-1.5 text-sm"
        value={body}
        onChange={(event) => setBody(event.target.value)}
        disabled={saveMutation.isPending || dismissMutation.isPending}
      />

      {proposal.quotes.length > 0 ? (
        <ScrollArea className="mb-2 max-h-28 rounded-md border border-border/60 bg-background/70">
          <div className="space-y-1 p-2 text-[11px] text-muted-foreground">
            {proposal.quotes.map((quote) => (
              <p
                key={quote.index}
                className={quote.anchor ? "font-medium text-foreground" : undefined}
              >
                {formatTime(quote.startMs)} {quote.text}
              </p>
            ))}
          </div>
        </ScrollArea>
      ) : (
        <p className="mb-2 text-[11px] text-muted-foreground">（无引用台词）</p>
      )}

      {error ? (
        <p className="mb-2 text-[11px] text-destructive">{error}</p>
      ) : null}

      <div className="flex flex-wrap gap-2">
        <Button
          size="sm"
          disabled={saveMutation.isPending || dismissMutation.isPending}
          onClick={() => saveMutation.mutate()}
        >
          {saveMutation.isPending ? "保存中…" : "确认保存"}
        </Button>
        <Button
          size="sm"
          variant="outline"
          disabled={saveMutation.isPending || dismissMutation.isPending}
          onClick={() => dismissMutation.mutate()}
        >
          取消
        </Button>
      </div>
    </div>
  );
}
