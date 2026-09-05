import { useQuery } from "@tanstack/react-query";

import { getMediaToolStatus } from "../api";

/** ffprobe presence for settings/degrade UI. No media file needed. */
export function useMediaToolStatus(enabled = true) {
  return useQuery({
    queryKey: ["media-tool-status"],
    queryFn: getMediaToolStatus,
    enabled,
    retry: false,
    staleTime: Infinity,
  });
}
