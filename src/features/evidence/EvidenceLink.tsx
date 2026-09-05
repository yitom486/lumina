import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Children,
  Fragment,
  cloneElement,
  isValidElement,
  useState,
  type ReactNode,
} from "react";

import { inspectMedia } from "@/features/media/api";
import { resolveEpisodeFile } from "@/features/library/api";
import { usePlayerStore } from "@/features/player";
import { useProgressStore } from "@/features/player/progressStore";
import { errorMessage, formatTime } from "@/lib/format";
import { cn } from "@/lib/utils";

import {
  parseEvidenceSegments,
  resolveCitation,
  unverifiedReasonText,
  type EvidenceRef,
} from "./evidence";

const SKIP_DESCEND = new Set(["code", "pre", "a"]);

/**
 * Split string children on bracketed citations. Element children recurse
 * except code/pre/link (citations inside code or links never linkify).
 * Strings stay strings when citation-free, so rendering is unchanged.
 */
export function enhanceCitationChildren(children: ReactNode): ReactNode {
  return Children.map(children, (child) => {
    if (typeof child === "string") {
      const segments = parseEvidenceSegments(child);
      if (
        segments.length === 1 &&
        segments[0]?.kind === "text"
      ) {
        return child;
      }
      return segments.map((segment, index) =>
        segment.kind === "text" ? (
          <Fragment key={index}>{segment.text}</Fragment>
        ) : (
          <EvidenceCitation key={index} cite={segment.ref} />
        ),
      );
    }
    if (
      isValidElement(child) &&
      typeof child.type === "string" &&
      !SKIP_DESCEND.has(child.type)
    ) {
      const inner = (child.props as { children?: ReactNode }).children;
      if (inner == null) return child;
      return cloneElement(child, undefined, enhanceCitationChildren(inner));
    }
    return child;
  });
}

function useResolvedCitation(ref: EvidenceRef) {
  const currentFile = usePlayerStore((s) => s.currentFile);
  const durationMs = usePlayerStore((s) => s.durationMs);
  const queryClient = useQueryClient();
  return useQuery({
    queryKey: ["evidence", currentFile, ref.label],
    queryFn: () =>
      resolveCitation(ref, {
        currentFile,
        durationMs,
        resolveEpisodeFile: (season, episode) =>
          currentFile
            ? resolveEpisodeFile(currentFile, season, episode).catch(
                () => null,
              )
            : Promise.resolve(null),
        fetchTargetDurationMs: (path) =>
          queryClient
            .fetchQuery({
              queryKey: ["mediaInfo", path],
              queryFn: () => inspectMedia(path),
              staleTime: Infinity,
            })
            .then(
              (info) => info.durationMs ?? null,
              () => null,
            ),
      }),
    staleTime: Infinity,
    retry: false,
  });
}

function formatRefTime(ref: EvidenceRef): string {
  const start = formatTime(ref.startMs);
  if (ref.endMs == null || ref.endMs <= ref.startMs) return start;
  return `${start}–${formatTime(ref.endMs)}`;
}

/**
 * Verified citation → inline jump button. Anything else → plain,
 * explicitly unverified text. Never auto-upgrades model output.
 */
export function EvidenceCitation({ cite: ref }: { cite: EvidenceRef }) {
  const [confirming, setConfirming] = useState(false);
  const [jumpError, setJumpError] = useState<string | null>(null);
  const currentFileNow = usePlayerStore((s) => s.currentFile);
  const resolved = useResolvedCitation(ref);

  // Loading or unverified: render the original text, never a link.
  if (!resolved.data || !resolved.data.verified) {
    const reason = resolved.data
      ? unverifiedReasonText(resolved.data.reason)
      : null;
    return (
      <span
        className="text-muted-foreground underline decoration-dotted underline-offset-2"
        title={reason ?? "正在校验引用…"}
      >
        {ref.label}
      </span>
    );
  }

  const { mediaPath } = resolved.data;
  const sameMedia = currentFileNow === mediaPath;

  if (sameMedia && !confirming) {
    return (
      <button
        type="button"
        className="font-medium text-sky-400 underline underline-offset-2 hover:text-sky-300"
        title={`跳转到 ${formatRefTime(ref)}`}
        onClick={() => {
          void usePlayerStore.getState().seek(ref.startMs);
        }}
      >
        {ref.label}
      </button>
    );
  }

  return (
    <span className="inline-flex flex-wrap items-center gap-1">
      <button
        type="button"
        className={cn(
          "font-medium text-sky-400 underline underline-offset-2 hover:text-sky-300",
        )}
        title={
          sameMedia
            ? `跳转到 ${formatRefTime(ref)}`
            : `切换媒体后跳转到 ${formatRefTime(ref)}`
        }
        onClick={() => {
          if (sameMedia) {
            void usePlayerStore.getState().seek(ref.startMs);
            return;
          }
          setJumpError(null);
          setConfirming(true);
        }}
      >
        {ref.label}
      </button>
      {confirming && !sameMedia ? (
        <span className="inline-flex items-center gap-1 rounded-md border border-border px-1.5 py-0.5 text-xs">
          <span className="text-muted-foreground">
            切换媒体并保存当前位置？
          </span>
          <button
            type="button"
            className="font-medium text-sky-400 hover:text-sky-300"
            onClick={() => {
              void (async () => {
                try {
                  const store = usePlayerStore.getState();
                  if (store.currentFile) {
                    useProgressStore
                      .getState()
                      .saveProgress(
                        store.currentFile,
                        store.currentTimeMs,
                      );
                  }
                  const opened = await store.openPath(mediaPath, {
                    rebuildPlaylist: true,
                  });
                  if (!opened) return;
                  await store.seek(ref.startMs);
                  setConfirming(false);
                } catch (error) {
                  setJumpError(errorMessage(error));
                }
              })();
            }}
          >
            切换
          </button>
          <button
            type="button"
            className="text-muted-foreground hover:text-foreground"
            onClick={() => {
              setConfirming(false);
              setJumpError(null);
            }}
          >
            取消
          </button>
          {jumpError ? (
            <span className="text-destructive">{jumpError}</span>
          ) : null}
        </span>
      ) : null}
    </span>
  );
}
