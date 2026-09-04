import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Channel } from "@tauri-apps/api/core";

import { Button } from "@/components/ui/button";
import { errorMessage } from "@/lib/format";
import {
  getYtdlCookieStatus,
  getYtdlStatus,
  installYtdl,
  listBrowserProfiles,
  pickCookiesFile,
  setYtdlCookies,
  testYtdlCookies,
  type CookieBrowser,
  type CookieMode,
  type YtdlInstallEvent,
} from "@/features/ytdl";

const BROWSERS: { id: CookieBrowser; label: string }[] = [
  { id: "chrome", label: "Chrome" },
  { id: "edge", label: "Edge" },
  { id: "firefox", label: "Firefox" },
];

function recommendsCookieFile(message: string | undefined): boolean {
  if (!message) return false;
  return (
    message.includes("加密") ||
    message.includes("Cookie 文件") ||
    message.includes("任务管理器")
  );
}

export function OnlineSourceSettings() {
  const queryClient = useQueryClient();
  const statusQuery = useQuery({
    queryKey: ["ytdl-status"],
    queryFn: getYtdlStatus,
  });
  const cookieQuery = useQuery({
    queryKey: ["ytdl-cookies"],
    queryFn: getYtdlCookieStatus,
  });

  const cookie = cookieQuery.data;
  const selectedBrowser = cookie?.browser ?? "chrome";
  const browserMode = cookie?.mode === "browser";
  const fileMode = cookie?.mode === "file";

  const profilesQuery = useQuery({
    queryKey: ["ytdl-browser-profiles", selectedBrowser],
    queryFn: () => listBrowserProfiles(selectedBrowser),
    enabled: browserMode,
  });

  const saveCookies = useMutation({
    mutationFn: setYtdlCookies,
    onSuccess: (status) => {
      queryClient.setQueryData(["ytdl-cookies"], status);
    },
  });

  const install = useMutation({
    mutationFn: async () => {
      const channel = new Channel<YtdlInstallEvent>();
      return installYtdl(channel);
    },
    onSuccess: (status) => {
      queryClient.setQueryData(["ytdl-status"], status);
    },
  });

  const testCookies = useMutation({
    mutationFn: testYtdlCookies,
  });

  const ytdl = statusQuery.data;
  const busy =
    saveCookies.isPending || install.isPending || testCookies.isPending;

  const persist = (input: {
    mode: CookieMode;
    browser?: CookieBrowser;
    browserProfile?: string | null;
    filePath?: string | null;
  }) =>
    saveCookies.mutateAsync({
      mode: input.mode,
      browser: input.browser ?? cookie?.browser ?? "chrome",
      browserProfile:
        input.browserProfile === undefined
          ? (cookie?.browserProfile ?? null)
          : input.browserProfile,
      filePath:
        input.filePath === undefined ? (cookie?.filePath ?? null) : input.filePath,
    });

  const setMode = (mode: CookieMode) => {
    void persist({
      mode,
      filePath: mode === "file" ? cookie?.filePath ?? null : null,
      browserProfile: mode === "browser" ? cookie?.browserProfile ?? null : null,
    });
  };

  const setBrowser = (browser: CookieBrowser) => {
    void persist({
      mode: "browser",
      browser,
      browserProfile: null,
      filePath: null,
    }).then(() => {
      void queryClient.invalidateQueries({
        queryKey: ["ytdl-browser-profiles", browser],
      });
    });
  };

  const setProfile = (browserProfile: string) => {
    void persist({
      mode: "browser",
      browser: selectedBrowser,
      browserProfile,
      filePath: null,
    });
  };

  const importFile = async () => {
    const path = await pickCookiesFile();
    if (!path) return;
    await persist({
      mode: "file",
      browserProfile: null,
      filePath: path,
    });
    await testCookies.mutateAsync();
  };

  const profiles = profilesQuery.data ?? [];

  return (
    <div className="mt-3 space-y-2 rounded-md border border-border/70 bg-muted/30 p-3 text-left text-xs">
      <p className="font-medium text-foreground">在线解析 / 登录态</p>
      <p className="text-muted-foreground">
        {ytdl?.message ?? "正在检查解析组件…"}
      </p>
      {ytdl && !ytdl.cliReady && ytdl.installSupported ? (
        <Button
          type="button"
          size="sm"
          variant="secondary"
          disabled={busy}
          onClick={() => void install.mutateAsync()}
        >
          {install.isPending ? "正在安装…" : "安装在线解析组件"}
        </Button>
      ) : null}

      <p className="pt-1 text-muted-foreground">
        {cookie?.message ??
          "登录态仅本机使用，不会交给 AI。公开视频可不导入。"}
      </p>

      <div className="space-y-2 rounded-md border border-border bg-background/60 px-3 py-2 text-[11px] leading-relaxed">
        <p className="font-medium text-foreground">
          Chrome 无法直读时：三步导入 cookies.txt
        </p>
        <ol className="list-decimal space-y-1 pl-4 text-muted-foreground">
          <li>扩展商店安装可导出 Netscape cookies.txt 的扩展</li>
          <li>在 youtube.com（或 B 站）用目标账号登录后导出文件</li>
          <li>点下方按钮导入 →「测试登录态是否可读」→ 再打开链接</li>
        </ol>
        <p className="text-muted-foreground">
          说明：这不是 Google / B 站官方 OAuth；只是把浏览器里的登录态文件交给本机解析。路径会记住，过期后再导出一次即可。
        </p>
        <Button
          type="button"
          size="sm"
          disabled={busy}
          onClick={() => void importFile()}
        >
          {fileMode ? "重新导入 cookies.txt" : "导入 cookies.txt（推荐）"}
        </Button>
      </div>

      <details className="rounded-md border border-border/60 px-2 py-1.5">
        <summary className="cursor-pointer text-[11px] text-muted-foreground">
          高级：尝试从浏览器直接读取（多数 Chrome 会失败）
        </summary>
        <div className="mt-2 space-y-2">
          <div className="flex flex-wrap gap-1.5">
            <Button
              type="button"
              size="sm"
              variant={cookie?.mode === "none" ? "default" : "outline"}
              disabled={busy}
              onClick={() => setMode("none")}
            >
              不使用
            </Button>
            {BROWSERS.map((b) => (
              <Button
                key={b.id}
                type="button"
                size="sm"
                variant={
                  cookie?.mode === "browser" && cookie.browser === b.id
                    ? "default"
                    : "outline"
                }
                disabled={busy}
                onClick={() => setBrowser(b.id)}
              >
                {b.label}
              </Button>
            ))}
          </div>

          {browserMode ? (
            <div className="space-y-1.5">
              <p className="text-[11px] text-muted-foreground">选择配置档案</p>
              {profilesQuery.isLoading ? (
                <p className="text-muted-foreground">正在扫描本机配置档案…</p>
              ) : profiles.length === 0 ? (
                <p className="text-muted-foreground">
                  未找到可用配置档案，请改用上方导入 cookies.txt。
                </p>
              ) : (
                <div className="flex flex-wrap gap-1.5">
                  {profiles.map((profile) => (
                    <Button
                      key={profile.id}
                      type="button"
                      size="sm"
                      variant={
                        cookie?.browserProfile === profile.id
                          ? "default"
                          : "outline"
                      }
                      disabled={busy}
                      onClick={() => setProfile(profile.id)}
                    >
                      {profile.label}
                    </Button>
                  ))}
                </div>
              )}
            </div>
          ) : null}
        </div>
      </details>

      {cookie?.mode !== "none" ? (
        <div className="flex flex-wrap items-center gap-2 pt-1">
          <Button
            type="button"
            size="sm"
            variant="secondary"
            disabled={busy || !ytdl?.cliReady}
            onClick={() => void testCookies.mutateAsync()}
          >
            {testCookies.isPending ? "测试中…" : "测试登录态是否可读"}
          </Button>
          {testCookies.data ? (
            <span
              className={
                testCookies.data.ok ? "text-emerald-500" : "text-destructive"
              }
            >
              {testCookies.data.message}
            </span>
          ) : null}
        </div>
      ) : null}

      {testCookies.data &&
      !testCookies.data.ok &&
      recommendsCookieFile(testCookies.data.message) ? (
        <p className="text-[11px] text-amber-600 dark:text-amber-400">
          浏览器直读失败时，请按上方三步导入 cookies.txt（不要期待 Google
          一键授权）。
        </p>
      ) : null}

      {(saveCookies.error || install.error || testCookies.error) && (
        <p className="text-destructive">
          {errorMessage(
            saveCookies.error ?? install.error ?? testCookies.error,
          )}
        </p>
      )}
    </div>
  );
}
