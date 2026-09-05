import { create } from "zustand";

export type AskAboutRequest = {
  /** Media time the question is about (cue start / chapter start). */
  anchorMs: number;
  /** Prefilled editable draft (Chinese preset + quoted context). */
  text: string;
  nonce: number;
};

type AskAboutStore = {
  /** Pending shortcut-ask; consumed once by AcpPanel. Not persisted. */
  request: AskAboutRequest | null;
  askAbout: (anchorMs: number, text: string) => void;
  consume: () => AskAboutRequest | null;
};

let nonce = 0;

export const useAskAboutStore = create<AskAboutStore>()((set, get) => ({
  request: null,
  askAbout: (anchorMs, text) => {
    nonce += 1;
    set({ request: { anchorMs, text, nonce } });
  },
  consume: () => {
    const request = get().request;
    if (request) set({ request: null });
    return request;
  },
}));

/** A couple of Chinese presets; free input stays fully editable. */
export function explainSegmentPreset(cueText: string, startMsLabel: string): string {
  const quote = cueText.replace(/\s+/g, " ").slice(0, 60);
  return `解释这一段（${startMsLabel}）：${quote}`;
}

export function summarizeChapterPreset(title: string, rangeLabel: string): string {
  return `总结本章「${title}」（${rangeLabel}）`;
}
