import { create } from "zustand";

import type { Cue } from "@/features/transcript/types";

import {
  cueIndicesInListRange,
  mergeCueIndices,
  toggleCueIndex,
} from "./noteQuoteSelection";
import type { QuoteMode } from "./types";

type PickFromTranscriptInput = {
  mediaPath: string;
  subtitleChoiceId: string;
  cues: Cue[];
  listIndex: number;
  shiftKey: boolean;
  asAnchor?: boolean;
};

const emptyQuotePicker = {
  lastPickedCueIndex: null as number | null,
  lastPickedListIndex: null as number | null,
};

/** Ephemeral state for the note currently being composed — not persisted until save. */
type NoteComposeState = {
  mediaPath: string | null;
  body: string;
  includeQuotes: boolean;
  quoteMode: QuoteMode;
  selectedIndices: number[];
  anchorCueIndex: number | null;
  lastPickedCueIndex: number | null;
  lastPickedListIndex: number | null;
  setBody: (body: string) => void;
  setIncludeQuotes: (includeQuotes: boolean) => void;
  syncMedia: (mediaPath: string) => void;
  setQuoteMode: (mode: QuoteMode) => void;
  setSelectedIndices: (indices: number[]) => void;
  toggleAtListIndex: (
    cues: Cue[],
    listIndex: number,
    shiftKey: boolean,
  ) => void;
  setAnchorCueIndex: (cueIndex: number | null) => void;
  clearQuoteSelection: () => void;
  /** Start a brand-new compose session (empty body + quotes). */
  resetCompose: () => void;
  /** After save: clear body but keep quote picks for another note on the same lines. */
  resetComposeKeepQuotes: () => void;
  pickFromTranscript: (input: PickFromTranscriptInput) => void;
};

export const useNoteComposeStore = create<NoteComposeState>((set, get) => ({
  mediaPath: null,
  body: "",
  includeQuotes: true,
  quoteMode: "auto",
  selectedIndices: [],
  anchorCueIndex: null,
  ...emptyQuotePicker,

  setBody: (body) => set({ body }),

  setIncludeQuotes: (includeQuotes) => set({ includeQuotes }),

  syncMedia: (mediaPath) => {
    if (get().mediaPath === mediaPath) return;
    set({
      mediaPath,
      body: "",
      includeQuotes: true,
      quoteMode: "auto",
      selectedIndices: [],
      anchorCueIndex: null,
      ...emptyQuotePicker,
    });
  },

  setQuoteMode: (quoteMode) => set({ quoteMode }),

  setSelectedIndices: (selectedIndices) =>
    set({ selectedIndices, quoteMode: "manual" }),

  toggleAtListIndex: (cues, listIndex, shiftKey) => {
    const cue = cues[listIndex];
    if (!cue) return;

    if (shiftKey && get().lastPickedListIndex != null) {
      const range = cueIndicesInListRange(
        cues,
        get().lastPickedListIndex as number,
        listIndex,
      );
      set({
        selectedIndices: mergeCueIndices(get().selectedIndices, range),
        quoteMode: "manual",
        includeQuotes: true,
        lastPickedCueIndex: cue.index,
        lastPickedListIndex: listIndex,
      });
      return;
    }

    const selectedIndices = toggleCueIndex(get().selectedIndices, cue.index);
    set({
      selectedIndices,
      quoteMode: "manual",
      includeQuotes: true,
      lastPickedCueIndex: cue.index,
      lastPickedListIndex: listIndex,
      anchorCueIndex:
        get().anchorCueIndex === cue.index && !selectedIndices.includes(cue.index)
          ? null
          : get().anchorCueIndex,
    });
  },

  setAnchorCueIndex: (anchorCueIndex) => set({ anchorCueIndex }),

  clearQuoteSelection: () =>
    set({
      selectedIndices: [],
      anchorCueIndex: null,
      ...emptyQuotePicker,
    }),

  resetCompose: () =>
    set({
      body: "",
      includeQuotes: true,
      quoteMode: "auto",
      selectedIndices: [],
      anchorCueIndex: null,
      ...emptyQuotePicker,
    }),

  resetComposeKeepQuotes: () => set({ body: "" }),

  pickFromTranscript: ({
    mediaPath,
    subtitleChoiceId: _subtitleChoiceId,
    cues,
    listIndex,
    shiftKey,
    asAnchor,
  }) => {
    get().syncMedia(mediaPath);
    const cue = cues[listIndex];
    if (!cue) return;

    if (shiftKey && get().lastPickedListIndex != null) {
      const range = cueIndicesInListRange(
        cues,
        get().lastPickedListIndex as number,
        listIndex,
      );
      set({
        quoteMode: "manual",
        includeQuotes: true,
        selectedIndices: mergeCueIndices(get().selectedIndices, range),
        lastPickedCueIndex: cue.index,
        lastPickedListIndex: listIndex,
        anchorCueIndex: asAnchor ? cue.index : get().anchorCueIndex,
      });
      return;
    }

    set({
      quoteMode: "manual",
      includeQuotes: true,
      selectedIndices: mergeCueIndices(get().selectedIndices, [cue.index]),
      lastPickedCueIndex: cue.index,
      lastPickedListIndex: listIndex,
      anchorCueIndex: asAnchor ? cue.index : get().anchorCueIndex,
    });
  },
}));

/** @deprecated Use useNoteComposeStore */
export const useNoteQuoteDraftStore = useNoteComposeStore;
