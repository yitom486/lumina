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
    // Verify the imported file immediately.
    await testCookies.mutateAsync();
  };

  const profiles = profilesQuery.data ?? [];
  const showCookieFileGuide =
    recommendsCookieFile(testCookies.data?.message) ||
    (browserMode && testCookies.data && !testCookies.data.ok);

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
        {cookie?.message ?? "登录态可选；仅本机使用，不会交给 AI。"}
      </p>

      {showCookieFileGuide ? (
        <div className="space-y-2 rounded-md border border-amber-500/40 bg-amber-500/10 px-3 py-2 text-[11px] leading-relaxed text-foreground">
          <p className="font-medium">推荐处理：导入 Cookie 文件</p>
          <ol className="list-decimal space-y-1 pl-4 text-muted-foreground">
            <li>在 Chrome 扩展商店安装可导出 Netscape cookies.txt 的扩展</li>
            <li>打开 youtube.com，用目标账号登录后，导出 cookies.txt</li>
            <li>回到这里点「导入 Cookie 文件」，再点「测试登录态是否可读」</li>
            <li>测试通过后再打开视频链接</li>
          </ol>
          <Button
            type="button"
            size="sm"
            disabled={busy}
            onClick={() => void importFile()}
          >
            选择并导入 cookies.txt
          </Button>
        </div>
      ) : (
        <p className="text-[11px] leading-relaxed text-muted-foreground">
          新版 Chrome/Edge 常因系统加密无法直接读取。多账号请用不同配置档案导出；同一档案内切换的
          Google 账号无法分别识别。
        </p>
      )}

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
        <Button
          type="button"
          size="sm"
          variant={fileMode ? "default" : "outline"}
          disabled={busy}
          onClick={() => void importFile()}
        >
          导入 Cookie 文件
        </Button>
      </div>

      {browserMode ? (
        <div className="space-y-1.5 pt-1">
          <p className="text-[11px] text-muted-foreground">选择配置档案</p>
          {profilesQuery.isLoading ? (
            <p className="text-muted-foreground">正在扫描本机配置档案…</p>
          ) : profiles.length === 0 ? (
            <p className="text-muted-foreground">
              未找到可用配置档案，请改用「导入 Cookie 文件」。
            </p>
          ) : (
            <div className="flex flex-wrap gap-1.5">
              {profiles.map((profile) => (
                <Button
                  key={profile.id}
                  type="button"
                  size="sm"
                  variant={
                    cookie?.browserProfile === profile.id ? "default" : "outline"
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
