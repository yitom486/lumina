export type NoteQuote = {
  index: number;
  startMs: number;
  endMs: number;
  text: string;
  anchor: boolean;
};

export type Note = {
  id: string;
  mediaPath: string;
  positionMs: number;
  body: string;
  quotes: NoteQuote[];
  createdAt: string;
  updatedAt: string;
};

export type QuoteMode = "auto" | "manual";

export type NoteCreate = {
  mediaPath: string;
  positionMs: number;
  body: string;
  subtitleChoiceId?: string | null;
  anchorCueIndex?: number | null;
  quoteCueIndices?: number[] | null;
  quoteHint?: string | null;
  includeQuotes?: boolean | null;
};

export type NotePreviewQuotes = {
  mediaPath: string;
  positionMs: number;
  subtitleChoiceId?: string | null;
  anchorCueIndex?: number | null;
  quoteCueIndices?: number[] | null;
  quoteHint?: string | null;
};

export type NoteUpdate = {
  id: string;
  body?: string;
  positionMs?: number;
};
