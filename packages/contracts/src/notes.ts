/** Note DTOs mirrored from Rust (camelCase JSON). */

export type NoteQuote = {
  index: number;
  startMs: number;
  endMs: number;
  text: string;
  anchor: boolean;
};

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
