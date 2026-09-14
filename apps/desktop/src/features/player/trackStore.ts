/** Shared subtitle/audio track selection (player bar + transcript stay in sync). */

import { create } from "zustand";
import { persist } from "zustand/middleware";

import type { SubtitleChoice } from "@lumina/contracts";

import {
  type DirectoryTrackPreference,
  audioPreferenceFromTrack,
  mediaDirectoryKey,
  subtitlePreferenceFromChoice,
} from "@lumina/player-ui";

type TrackState = {
  /** 选中的字幕轨：文稿/AI/翻译的数据源。显示开关从不清空它，
   * 空只代表"无可用轨"。 */
  subtitleChoiceId: string | null;
  /** 只管 mpv overlay 是否显示，不影响选中轨与 AI 可读性。 */
  subtitleVisible: boolean;
  audioStreamIndex: number | null;
  directoryPrefs: Record<string, DirectoryTrackPreference>;
  setSubtitleChoiceId: (id: string | null) => void;
  setSubtitleVisible: (visible: boolean) => void;
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
      subtitleVisible: true,
      audioStreamIndex: null,
      directoryPrefs: {},

      setSubtitleChoiceId: (subtitleChoiceId) => set({ subtitleChoiceId }),
      setSubtitleVisible: (subtitleVisible) => set({ subtitleVisible }),
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
        subtitleVisible: state.subtitleVisible,
      }),
      merge: (persisted, current) => {
        const saved = (persisted ?? {}) as Partial<
          Pick<TrackState, "directoryPrefs" | "subtitleVisible">
        >;
        return {
          ...current,
          directoryPrefs:
            saved.directoryPrefs ?? current.directoryPrefs ?? {},
          subtitleVisible: saved.subtitleVisible ?? true,
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
