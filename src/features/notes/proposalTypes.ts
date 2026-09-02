import type { NoteQuote } from "./types";

export type VideoAnnotationProposal = {
  proposalId: string;
  mediaPath: string;
  positionMs: number;
  body: string;
  subtitleChoiceId?: string | null;
  anchorCueIndex?: number | null;
  quoteCueIndices?: number[] | null;
  quoteHint?: string | null;
  includeQuotes: boolean;
  quotes: NoteQuote[];
  previewMarkdown: string;
  createdAtMs: number;
};
