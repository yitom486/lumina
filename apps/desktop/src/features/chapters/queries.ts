export function chapterSegmentationKey(
  mediaPath: string | null | undefined,
  episodeKey: string | null | undefined,
) {
  return ["chapter-segmentation", mediaPath ?? null, episodeKey ?? null] as const;
}

