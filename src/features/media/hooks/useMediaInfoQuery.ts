import { useQuery } from "@tanstack/react-query";

import { usePlayerStore } from "@/features/player";

import { inspectMedia } from "../api";

export function useMediaInfoQuery() {
  const path = usePlayerStore((s) => s.currentFile);
  const status = usePlayerStore((s) => s.status);

  const enabled =
    Boolean(path) &&
    status !== "Idle" &&
    status !== "Loading" &&
    status !== "Error";

  return useQuery({
    queryKey: ["mediaInfo", path],
    queryFn: () => inspectMedia(path as string),
    enabled,
    retry: false,
    staleTime: Infinity,
  });
}
