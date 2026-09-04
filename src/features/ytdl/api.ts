/** Optional yt-dlp (online source) — mirrors ASR on-demand pattern. */

import { Channel, invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

import type { MediaChapter } from "@/features/media/types";

export type YtdlStatus = {
  available: boolean;
  cliReady: boolean;
  cliPath: string | null;
  version: string | null;
  installSupported: boolean;
  message: string;
};

export type CookieMode = "none" | "browser" | "file";
export type CookieBrowser = "chrome" | "edge" | "firefox";

export type YtdlCookieStatus = {
  mode: CookieMode;
  browser: CookieBrowser;
  browserProfile: string | null;
  filePath: string | null;
  message: string;
};

export type YtdlCookieConfigInput = {
  mode: CookieMode;
  browser?: CookieBrowser | null;
  browserProfile?: string | null;
  filePath?: string | null;
};

export type BrowserProfileOption = {
  id: string;
  label: string;
};

export type YtdlCookieTestResult = {
  ok: boolean;
  message: string;
};

export type YtdlFormat = {
  formatId: string;
  ext?: string | null;
  height?: number | null;
  width?: number | null;
  fps?: number | null;
  vcodec?: string | null;
  acodec?: string | null;
  tbr?: number | null;
  formatNote?: string | null;
  url?: string | null;
};

export type YtdlSubtitleTrack = {
  language: string;
  ext?: string | null;
  name?: string | null;
};

export type YtdlResolveResult = {
  mediaId: string;
  title?: string | null;
  durationMs?: number | null;
  webpageUrl?: string | null;
  extractor?: string | null;
  chapters: MediaChapter[];
  formats: YtdlFormat[];
  subtitles: YtdlSubtitleTrack[];
  recommendedUrl?: string | null;
  recommendedFormatId?: string | null;
};

export type YtdlInstallEvent =
  | {
      type: "Progress";
      stage: string;
      message: string;
      downloaded?: number | null;
      total?: number | null;
    }
  | { type: "Finished"; status: YtdlStatus };

export function getYtdlStatus(): Promise<YtdlStatus> {
  return invoke<YtdlStatus>("ytdl_status");
}

export function getYtdlCookieStatus(): Promise<YtdlCookieStatus> {
  return invoke<YtdlCookieStatus>("ytdl_cookie_status");
}

export function setYtdlCookies(
  config: YtdlCookieConfigInput,
): Promise<YtdlCookieStatus> {
  return invoke<YtdlCookieStatus>("ytdl_set_cookies", { config });
}

export function listBrowserProfiles(
  browser: CookieBrowser,
): Promise<BrowserProfileOption[]> {
  return invoke<BrowserProfileOption[]>("ytdl_list_browser_profiles", {
    browser,
  });
}

export function testYtdlCookies(): Promise<YtdlCookieTestResult> {
  return invoke<YtdlCookieTestResult>("ytdl_test_cookies");
}

export function installYtdl(
  onEvent: Channel<YtdlInstallEvent>,
): Promise<YtdlStatus> {
  return invoke<YtdlStatus>("ytdl_install", { onEvent });
}

export function resolveYtdlUrl(url: string): Promise<YtdlResolveResult> {
  return invoke<YtdlResolveResult>("ytdl_resolve", { url });
}

export async function pickCookiesFile(): Promise<string | null> {
  const selected = await open({
    multiple: false,
    filters: [
      { name: "Cookies", extensions: ["txt"] },
      { name: "All", extensions: ["*"] },
    ],
  });
  if (!selected || Array.isArray(selected)) {
    return null;
  }
  return selected;
}
