/** Shared subtitle/audio track selection (player bar + transcript stay in sync). */

import { create } from "zustand";

type TrackState = {
  subtitleChoiceId: string | null;
  audioStreamIndex: number | null;
  setSubtitleChoiceId: (id: string | null) => void;
  setAudioStreamIndex: (index: number | null) => void;
  resetForFile: () => void;
};

export const useTrackStore = create<TrackState>((set) => ({
  subtitleChoiceId: null,
  audioStreamIndex: null,
  setSubtitleChoiceId: (subtitleChoiceId) => set({ subtitleChoiceId }),
  setAudioStreamIndex: (audioStreamIndex) => set({ audioStreamIndex }),
  resetForFile: () => set({ subtitleChoiceId: null, audioStreamIndex: null }),
}));
