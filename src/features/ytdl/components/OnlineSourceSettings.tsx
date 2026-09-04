import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Channel } from "@tauri-apps/api/core";

import { Button } from "@/components/ui/button";
import { errorMessage } from "@/lib/format";
import {
  getYtdlCookieStatus,
  getYtdlStatus,
  installYtdl,
  pickCookiesFile,
  setYtdlCookies,
  type CookieBrowser,
  type CookieMode,
  type YtdlInstallEvent,
} from "@/features/ytdl";

const BROWSERS: { id: CookieBrowser; label: string }[] = [
  { id: "chrome", label: "Chrome" },
  { id: "edge", label: "Edge" },
  { id: "firefox", label: "Firefox" },
];

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

  const cookie = cookieQuery.data;
  const ytdl = statusQuery.data;
  const busy = saveCookies.isPending || install.isPending;

  const setMode = (mode: CookieMode) => {
    void saveCookies.mutateAsync({
      mode,
      browser: cookie?.browser ?? "chrome",
      filePath: cookie?.filePath ?? null,
    });
  };

  const setBrowser = (browser: CookieBrowser) => {
    void saveCookies.mutateAsync({
      mode: "browser",
      browser,
      filePath: null,
    });
  };

  const importFile = async () => {
    const path = await pickCookiesFile();
    if (!path) return;
    await saveCookies.mutateAsync({
      mode: "file",
      browser: cookie?.browser ?? "chrome",
      filePath: path,
    });
  };

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
          variant={cookie?.mode === "file" ? "default" : "outline"}
          disabled={busy}
          onClick={() => void importFile()}
        >
          导入 Cookie 文件
        </Button>
      </div>
      {(saveCookies.error || install.error) && (
        <p className="text-destructive">
          {errorMessage(saveCookies.error ?? install.error)}
        </p>
      )}
    </div>
  );
}
