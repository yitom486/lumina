import { useQuery } from "@tanstack/react-query";

import { usePlayerStore } from "@/features/player";
import { isRemotePath } from "@/features/player/sessionStore";

import { inspectMedia } from "../api";
import { mediaInfoKey } from "../queries";

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
