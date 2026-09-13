import type { SubtitleChoice } from "@/features/transcript/types";

import { workspaceCwdFromMedia } from "../acp/cwd";

export type StoredSubtitlePreference =
  | { mode: "none" }
  | {
      mode: "embedded";
      language: string | null;
      streamIndex: number;
    }
  | {
      mode: "sidecar";
      language: string | null;
      /** File-name suffix after the video stem, e.g. `.zh.srt` or `.en.ass`. */
      sidecarToken: string | null;
    };

export type StoredAudioPreference = {
  language: string | null;
  streamIndex: number;
};

export type DirectoryTrackPreference = {
  subtitle: StoredSubtitlePreference | null;
  audio: StoredAudioPreference | null;
};

export function mediaDirectoryKey(mediaPath: string): string {
  const dir = workspaceCwdFromMedia(mediaPath);
  if (!dir) return "";
  return dir.replace(/\\/g, "/").toLowerCase();
}

function basename(path: string): string {
  const normalized = path.replace(/[/\\]+$/, "");
  const idx = Math.max(
    normalized.lastIndexOf("/"),
    normalized.lastIndexOf("\\"),
  );
  return idx >= 0 ? normalized.slice(idx + 1) : normalized;
}

function stemWithoutExt(fileName: string): string {
  const dot = fileName.lastIndexOf(".");
  return dot > 0 ? fileName.slice(0, dot) : fileName;
}

export function sidecarTokenForMedia(
  mediaPath: string,
  externalPath: string,
): string | null {
  const mediaStem = stemWithoutExt(basename(mediaPath));
  const sidecarName = basename(externalPath);
  if (sidecarName.startsWith(mediaStem)) {
    return sidecarName.slice(mediaStem.length) || null;
  }
  return sidecarName;
}

export function subtitlePreferenceFromChoice(
  choice: SubtitleChoice | null,
  mediaPath: string,
): StoredSubtitlePreference {
  if (!choice) return { mode: "none" };
  if (choice.source === "Embedded") {
    return {
      mode: "embedded",
      language: choice.language ?? null,
      streamIndex: choice.streamIndex ?? 0,
    };
  }
  return {
    mode: "sidecar",
    language: choice.language ?? null,
    sidecarToken: choice.externalPath
      ? sidecarTokenForMedia(mediaPath, choice.externalPath)
      : null,
  };
}

export function audioPreferenceFromTrack(track: {
  index: number;
  language?: string | null;
}): StoredAudioPreference {
  return {
    language: track.language ?? null,
    streamIndex: track.index,
  };
}

export function pickDefaultSubtitleId(
  choices: SubtitleChoice[],
): string | null {
  const text = choices.find((c) => c.supported);
  return text?.id ?? choices[0]?.id ?? null;
}

export function resolveSubtitleChoiceId(
  pref: StoredSubtitlePreference | null | undefined,
  choices: SubtitleChoice[],
  mediaPath: string,
): string | null {
  if (!pref) return pickDefaultSubtitleId(choices);
  if (pref.mode === "none") return null;
  if (choices.length === 0) return null;

  if (pref.mode === "embedded") {
    if (pref.language) {
      const byLang = choices.find(
        (c) =>
          c.source === "Embedded" &&
          (c.language ?? "und") === pref.language,
      );
      if (byLang) return byLang.id;
    }
    const byIndex = choices.find(
      (c) =>
        c.source === "Embedded" && c.streamIndex === pref.streamIndex,
    );
    if (byIndex) return byIndex.id;
    return pickDefaultSubtitleId(choices);
  }

  const mediaStem = stemWithoutExt(basename(mediaPath));
  if (pref.sidecarToken) {
    const expectedName = `${mediaStem}${pref.sidecarToken}`;
    const byToken = choices.find(
      (c) =>
        c.source === "Sidecar" &&
        c.externalPath &&
        basename(c.externalPath) === expectedName,
    );
    if (byToken) return byToken.id;
  }
  if (pref.language) {
    const byLang = choices.find(
      (c) =>
        c.source === "Sidecar" &&
        (c.language ?? "und") === pref.language,
    );
    if (byLang) return byLang.id;
  }
  return pickDefaultSubtitleId(choices);
}

export function resolveAudioStreamIndex(
  pref: StoredAudioPreference | null | undefined,
  tracks: { index: number; language?: string | null }[],
): number | null {
  if (tracks.length === 0) return null;
  if (!pref) return tracks[0]?.index ?? null;

  if (pref.language) {
    const byLang = tracks.find(
      (t) => (t.language ?? "und") === pref.language,
    );
    if (byLang) return byLang.index;
  }
  const byIndex = tracks.find((t) => t.index === pref.streamIndex);
  if (byIndex) return byIndex.index;
  return tracks[0]?.index ?? null;
}

export function resolveDirectoryTrackSelection(
  mediaPath: string,
  choices: SubtitleChoice[],
  audioTracks: { index: number; language?: string | null }[],
  directoryPrefs: Record<string, DirectoryTrackPreference>,
): { subtitleChoiceId: string | null; audioStreamIndex: number | null } {
  const key = mediaDirectoryKey(mediaPath);
  const pref = key ? directoryPrefs[key] : undefined;
  return {
    subtitleChoiceId: resolveSubtitleChoiceId(
      pref?.subtitle,
      choices,
      mediaPath,
    ),
    audioStreamIndex: resolveAudioStreamIndex(pref?.audio, audioTracks),
  };
}
