import { useQuery } from "@tanstack/react-query";

import { usePlayerStore } from "@/features/player";

import { inspectMedia } from "../api";
import { mediaInfoKey } from "../queries";

function isRemotePath(path: string): boolean {
  const lower = path.trim().toLowerCase();
  return lower.startsWith("https://") || lower.startsWith("http://");
}

export function useMediaInfoQuery() {
  const path = usePlayerStore((s) => s.currentFile);
  const status = usePlayerStore((s) => s.status);
  const sourceKind = usePlayerStore((s) => s.sourceKind);

  const remote =
    sourceKind === "remote" || (path != null && isRemotePath(path));

  const enabled =
    Boolean(path) &&
    !remote &&
    status !== "Idle" &&
    status !== "Loading" &&
    status !== "Error";

  return useQuery({
    queryKey: mediaInfoKey(path),
    queryFn: () => inspectMedia(path as string),
    enabled,
    retry: false,
    staleTime: Infinity,
  });
}
