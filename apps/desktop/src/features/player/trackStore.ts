/** Shared subtitle/audio track selection (player bar + transcript stay in sync). */

import { create } from "zustand";
import { persist } from "zustand/middleware";

import type { SubtitleChoice } from "@/features/transcript/types";

import {
  type DirectoryTrackPreference,
  audioPreferenceFromTrack,
  mediaDirectoryKey,
  subtitlePreferenceFromChoice,
} from "./trackPreferences";

type TrackState = {
  subtitleChoiceId: string | null;
  audioStreamIndex: number | null;
  directoryPrefs: Record<string, DirectoryTrackPreference>;
  setSubtitleChoiceId: (id: string | null) => void;
  setAudioStreamIndex: (index: number | null) => void;
  rememberSubtitleForMedia: (
    mediaPath: string,
    choice: SubtitleChoice | null,
  ) => void;
  rememberAudioForMedia: (
    mediaPath: string,
    track: { index: number; language?: string | null },
  ) => void;
};

export const useTrackStore = create<TrackState>()(
  persist(
    (set) => ({
      subtitleChoiceId: null,
      audioStreamIndex: null,
      directoryPrefs: {},

      setSubtitleChoiceId: (subtitleChoiceId) => set({ subtitleChoiceId }),
      setAudioStreamIndex: (audioStreamIndex) => set({ audioStreamIndex }),

      rememberSubtitleForMedia: (mediaPath, choice) => {
        const key = mediaDirectoryKey(mediaPath);
        if (!key) return;
        const subtitle = subtitlePreferenceFromChoice(choice, mediaPath);
        set((state) => ({
          directoryPrefs: {
            ...state.directoryPrefs,
            [key]: {
              audio: state.directoryPrefs[key]?.audio ?? null,
              subtitle,
            },
          },
        }));
      },

      rememberAudioForMedia: (mediaPath, track) => {
        const key = mediaDirectoryKey(mediaPath);
        if (!key) return;
        const audio = audioPreferenceFromTrack(track);
        set((state) => ({
          directoryPrefs: {
            ...state.directoryPrefs,
            [key]: {
              subtitle: state.directoryPrefs[key]?.subtitle ?? null,
              audio,
            },
          },
        }));
      },
    }),
    {
      name: "lumina-track-prefs",
      partialize: (state) => ({
        directoryPrefs: state.directoryPrefs,
      }),
      merge: (persisted, current) => {
        const saved = (persisted ?? {}) as Partial<
          Pick<TrackState, "directoryPrefs">
        >;
        return {
          ...current,
          directoryPrefs:
            saved.directoryPrefs ?? current.directoryPrefs ?? {},
        };
      },
    },
  ),
);

export function getDirectoryTrackPreference(
  mediaPath: string,
): DirectoryTrackPreference | undefined {
  const key = mediaDirectoryKey(mediaPath);
  if (!key) return undefined;
  return useTrackStore.getState().directoryPrefs[key];
}
