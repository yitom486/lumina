import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";

import { Button } from "@lumina/ui/button";

import { LANG_PRESETS } from "../../../../../../packages/transcript-ui/src/cueSelectors";
import {
  downloadSubtitleCandidate,
  getSubtitleProviderStatus,
  searchOnlineSubtitles,
  setSubtitleProviderKey,
  validateSubtitleProviderKey,
  type SubtitleCandidate,
} from "../api";

/** ISO 639-2/B bibliographic -> 639-1 for the common audio languages. */
const AUDIO_LANG_MAP: Record<string, string> = {
  chi: "zh",
  zho: "zh",
  eng: "en",
  kor: "ko",
  jpn: "ja",
  fre: "fr",
  fra: "fr",
  ger: "de",
  deu: "de",
  spa: "es",
  ita: "it",
  rus: "ru",
  por: "pt",
  ara: "ar",
  hin: "hi",
  tha: "th",
  vie: "vi",
  ind: "id",
  msa: "ms",
  tur: "tr",
  pol: "pl",
  nld: "nl",
  dut: "nl",
  swe: "sv",
  nor: "no",
  dan: "da",
  fin: "fi",
  ell: "el",
  gre: "el",
  heb: "he",
  ukr: "uk",
};

export function guessOriginalLang(audioLanguage: string | null): string | null {
  const raw = audioLanguage?.trim().toLowerCase() ?? "";
  if (raw === "") return null;
  if (/^[a-z]{2}$/.test(raw)) return raw;
  return AUDIO_LANG_MAP[raw] ?? null;
}

function fileStem(path: string): string {
  const base = path.split(/[/\\]/).pop() ?? path;
  const dot = base.lastIndexOf(".");
  return dot > 0 ? base.slice(0, dot) : base;
}

function formatSize(bytes: number): string {
  if (bytes <= 0) return "未知大小";
  if (bytes < 1024) return `${bytes} B`;
  return `${(bytes / 1024).toFixed(0)} KB`;
}

type Props = {
  mediaPath: string;
  audioLanguage: string | null;
  disabled: boolean;
  onDownloaded: (choiceId: string) => void;
};

export function OnlineSubtitleSection({
  mediaPath,
  audioLanguage,
  disabled,
  onDownloaded,
}: Props) {
  const [keyInput, setKeyInput] = useState("");
  const [keyBusy, setKeyBusy] = useState(false);
  const [checkBusy, setCheckBusy] = useState(false);
  const [keyMessage, setKeyMessage] = useState<string | null>(null);
  const [title, setTitle] = useState(() => fileStem(mediaPath));
  const [season, setSeason] = useState("");
  const [episode, setEpisode] = useState("");
  const [targetLang, setTargetLang] = useState("zh");
  const [candidates, setCandidates] = useState<SubtitleCandidate[] | null>(null);
  const [searchBusy, setSearchBusy] = useState(false);
  const [downloadBusy, setDownloadBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setCandidates(null);
    setError(null);
    setTitle(fileStem(mediaPath));
  }, [mediaPath]);

  const statusQuery = useQuery({
    queryKey: ["subtitleProviders"],
    queryFn: getSubtitleProviderStatus,
    staleTime: 60_000,
    retry: false,
  });

  const subdl = statusQuery.data?.find((p) => p.id === "subdl");
  const originalLang = guessOriginalLang(audioLanguage);
  const prefer = [
    ...new Set(
      [targetLang, originalLang, "en"].filter(
        (lang): lang is string => Boolean(lang),
      ),
    ),
  ];

  async function handleSaveKey() {
    if (keyBusy) return;
    setKeyBusy(true);
    setKeyMessage(null);
    try {
      await setSubtitleProviderKey("subdl", keyInput);
      await statusQuery.refetch();
      setKeyInput("");
      setKeyMessage("已保存（仅存本机）");
    } catch (err) {
      setKeyMessage(
        typeof err === "object" && err && "message" in err
          ? String((err as { message: string }).message)
          : String(err),
      );
    } finally {
      setKeyBusy(false);
    }
  }

  async function handleCheckKey() {
    if (checkBusy || keyInput.trim() === "") return;
    setCheckBusy(true);
    setKeyMessage(null);
    try {
      const result = await validateSubtitleProviderKey("subdl", keyInput);
      setKeyMessage(result.verified ? "SubDL Key 有效，可保存使用" : result.message);
    } catch (err) {
      setKeyMessage(
        typeof err === "object" && err && "message" in err
          ? String((err as { message: string }).message)
          : String(err),
      );
    } finally {
      setCheckBusy(false);
    }
  }

  async function handleSearch() {
    if (searchBusy || downloadBusy) return;
    setSearchBusy(true);
    setError(null);
    try {
      const found = await searchOnlineSubtitles(
        mediaPath,
        {
          title: title.trim() || null,
          tmdbId: null,
          season: season.trim() === "" ? null : Number(season),
          episode: episode.trim() === "" ? null : Number(episode),
        },
        prefer,
      );
      setCandidates(found);
      if (found.length === 0) setError("没有找到可用字幕，换个片名或语言试试");
    } catch (err) {
      setCandidates(null);
      setError(
        typeof err === "object" && err && "message" in err
          ? String((err as { message: string }).message)
          : String(err),
      );
    } finally {
      setSearchBusy(false);
    }
  }

  async function handleDownload(candidate: SubtitleCandidate) {
    if (searchBusy || downloadBusy) return;
    setDownloadBusy(true);
    setError(null);
    try {
      const result = await downloadSubtitleCandidate(mediaPath, candidate);
      onDownloaded(result.choiceId);
    } catch (err) {
      setError(
        typeof err === "object" && err && "message" in err
          ? String((err as { message: string }).message)
          : String(err),
      );
    } finally {
      setDownloadBusy(false);
    }
  }

  const busy = disabled || searchBusy || downloadBusy || keyBusy || checkBusy;

  return (
    <div className="flex flex-col gap-2 rounded-md border border-border/70 bg-muted/20 p-2">
      <p className="text-xs font-medium">在线字幕（仅存本机缓存，不写视频目录）</p>

      <div className="flex flex-wrap items-end gap-2">
        <label className="flex min-w-[10rem] flex-1 flex-col gap-1 text-xs">
          <span className="text-muted-foreground">
            SubDL Key{subdl?.hasKey ? "（已配置）" : "（未配置）"}
          </span>
          <input
            type="password"
            className="w-full rounded-md border border-border bg-background px-2 py-1.5 text-sm disabled:opacity-50"
            value={keyInput}
            disabled={busy}
            placeholder="粘贴 Key 后保存"
            aria-label="SubDL API key"
            onChange={(e) => setKeyInput(e.target.value)}
          />
        </label>
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={busy || keyInput.trim() === ""}
          onClick={() => void handleCheckKey()}
        >
          {checkBusy ? "检测中…" : "检测"}
        </Button>
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={busy || keyInput.trim() === ""}
          onClick={() => void handleSaveKey()}
        >
          {keyBusy ? "保存中…" : "保存 Key"}
        </Button>
      </div>
      {keyMessage ? (
        <p className="text-[11px] text-muted-foreground">{keyMessage}</p>
      ) : null}

      <div className="flex flex-wrap items-end gap-2">
        <label className="flex min-w-[10rem] flex-[2] flex-col gap-1 text-xs">
          <span className="text-muted-foreground">片名</span>
          <input
            className="w-full rounded-md border border-border bg-background px-2 py-1.5 text-sm disabled:opacity-50"
            value={title}
            disabled={busy}
            aria-label="Title to search subtitles for"
            onChange={(e) => setTitle(e.target.value)}
          />
        </label>
        <label className="flex w-16 flex-col gap-1 text-xs">
          <span className="text-muted-foreground">季</span>
          <input
            className="w-full rounded-md border border-border bg-background px-2 py-1.5 text-sm disabled:opacity-50"
            value={season}
            disabled={busy}
            inputMode="numeric"
            aria-label="Season number"
            onChange={(e) => setSeason(e.target.value)}
          />
        </label>
        <label className="flex w-16 flex-col gap-1 text-xs">
          <span className="text-muted-foreground">集</span>
          <input
            className="w-full rounded-md border border-border bg-background px-2 py-1.5 text-sm disabled:opacity-50"
            value={episode}
            disabled={busy}
            inputMode="numeric"
            aria-label="Episode number"
            onChange={(e) => setEpisode(e.target.value)}
          />
        </label>
        <label className="flex min-w-[7rem] flex-1 flex-col gap-1 text-xs">
          <span className="text-muted-foreground">优先语言</span>
          <select
            className="w-full rounded-md border border-border bg-background px-2 py-1.5 text-sm disabled:opacity-50"
            value={targetLang}
            disabled={busy}
            aria-label="Preferred subtitle language"
            onChange={(e) => setTargetLang(e.target.value)}
          >
            {LANG_PRESETS.map((lang) => (
              <option key={lang.value} value={lang.value}>
                {lang.label}
              </option>
            ))}
          </select>
        </label>
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={busy || title.trim() === ""}
          onClick={() => void handleSearch()}
        >
          {searchBusy ? "搜索中…" : "搜索字幕"}
        </Button>
      </div>
      <p className="text-[11px] leading-snug text-muted-foreground">
        排序：{prefer.join(" → ")}
        {originalLang ? "（含音轨原语）" : ""}；下载需手动点每条结果。
      </p>

      {error ? (
        <p className="text-xs text-muted-foreground">{error}</p>
      ) : null}

      {candidates && candidates.length > 0 ? (
        <ul className="flex max-h-44 flex-col gap-1 overflow-auto">
          {candidates.map((candidate, index) => (
            <li
              key={`${candidate.provider}-${candidate.language}-${candidate.releaseName}-${index}`}
              className="flex items-center gap-2 rounded-md border border-border/50 px-2 py-1 text-xs"
            >
              <span className="shrink-0 rounded bg-secondary px-1.5 py-0.5 font-medium">
                {candidate.language}
              </span>
              <span className="min-w-0 flex-1 truncate" title={candidate.releaseName}>
                {candidate.releaseName || candidate.format}
                <span className="ml-1 text-muted-foreground">
                  · {formatSize(candidate.sizeBytes)}
                  {candidate.cached ? " · 已缓存" : ""}
                </span>
              </span>
              <Button
                type="button"
                variant="ghost"
                size="sm"
                className="h-6 shrink-0 px-2 text-[11px]"
                disabled={busy}
                onClick={() => void handleDownload(candidate)}
              >
                {downloadBusy ? "下载中…" : candidate.cached ? "已下载" : "下载"}
              </Button>
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}
