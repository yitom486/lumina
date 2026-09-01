import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { useMemo, useState } from "react";

import { Button } from "@/components/ui/button";
import { errorMessage } from "@/lib/format";

import {
  applyTmdbMediaMatch,
  deleteMetadataCredential,
  getLibraryStatus,
  getMetadataCredentialStatus,
  listPendingMediaGroups,
  previewMediaMatch,
  saveMetadataCredentials,
  setManualMediaTitle,
  startLibraryWatch,
  stopLibraryWatch,
  validateMetadataCredentials,
} from "../api";
import { useLibrarySettingsStore } from "../settingsStore";
import type {
  CredentialKind,
  CredentialValidationResult,
  PendingMediaGroup,
  ResolverPreview,
} from "../types";

export function MediaLibraryPanel() {
  const queryClient = useQueryClient();
  const roots = useLibrarySettingsStore((state) => state.roots);
  const pollIntervalSecs = useLibrarySettingsStore((state) => state.pollIntervalSecs);
  const privacyAcknowledged = useLibrarySettingsStore((state) => state.privacyAcknowledged);
  const modelBaseUrl = useLibrarySettingsStore((state) => state.modelBaseUrl);
  const modelId = useLibrarySettingsStore((state) => state.modelId);
  const tmdbLanguage = useLibrarySettingsStore((state) => state.tmdbLanguage);
  const patchSettings = useLibrarySettingsStore((state) => state.patchSettings);
  const [error, setError] = useState<string | null>(null);
  const [titles, setTitles] = useState<Record<string, string>>({});
  const [previews, setPreviews] = useState<Record<string, ResolverPreview>>({});
  const [modelApiKey, setModelApiKey] = useState("");
  const [tmdbAccessToken, setTmdbAccessToken] = useState("");
  const [validation, setValidation] = useState<CredentialValidationResult | null>(null);

  const statusQuery = useQuery({ queryKey: ["library-status"], queryFn: getLibraryStatus });
  const pendingQuery = useQuery({
    queryKey: ["library-pending"],
    queryFn: listPendingMediaGroups,
    enabled: Boolean(statusQuery.data?.running),
  });
  const credentialStatusQuery = useQuery({
    queryKey: ["library-credential-status"],
    queryFn: getMetadataCredentialStatus,
  });
  const config = useMemo(
    () => ({
      privacyAcknowledged,
      // Old installations can continue to supply these environment variables.
      // Newly saved secrets are read from Windows Credential Manager first.
      model: { baseUrl: modelBaseUrl, modelId, apiKeyEnv: "LUMINA_METADATA_MODEL_API_KEY" },
      tmdb: { accessTokenEnv: "LUMINA_TMDB_ACCESS_TOKEN", language: tmdbLanguage },
    }),
    [modelBaseUrl, modelId, privacyAcknowledged, tmdbLanguage],
  );
  const refresh = async () => {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: ["library-status"] }),
      queryClient.invalidateQueries({ queryKey: ["library-pending"] }),
    ]);
  };
  const startMutation = useMutation({
    mutationFn: () => startLibraryWatch({ roots, pollIntervalSecs }),
    onSuccess: () => void refresh(),
    onError: (err) => setError(errorMessage(err)),
  });
  const stopMutation = useMutation({
    mutationFn: stopLibraryWatch,
    onSuccess: () => void refresh(),
    onError: (err) => setError(errorMessage(err)),
  });
  const saveCredentialsMutation = useMutation({
    mutationFn: () => saveMetadataCredentials({
      modelApiKey: modelApiKey || undefined,
      tmdbAccessToken: tmdbAccessToken || undefined,
    }),
    onSuccess: () => {
      setModelApiKey("");
      setTmdbAccessToken("");
      setValidation(null);
      void queryClient.invalidateQueries({ queryKey: ["library-credential-status"] });
    },
    onError: (err) => setError(errorMessage(err)),
  });
  const deleteCredentialMutation = useMutation({
    mutationFn: (kind: CredentialKind) => deleteMetadataCredential(kind),
    onSuccess: () => {
      setValidation(null);
      void queryClient.invalidateQueries({ queryKey: ["library-credential-status"] });
    },
    onError: (err) => setError(errorMessage(err)),
  });
  const validateCredentialsMutation = useMutation({
    mutationFn: () => validateMetadataCredentials({
      model: config.model,
      tmdb: config.tmdb,
    }),
    onSuccess: (result) => setValidation(result),
    onError: (err) => setError(errorMessage(err)),
  });

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-auto p-3 text-xs">
      <section className="space-y-2">
        <div className="flex items-center justify-between gap-2">
          <p className="font-medium">媒体库守护</p>
          <span className="text-muted-foreground">
            {statusQuery.data?.running ? "运行中" : "未启动"}
          </span>
        </div>
        <div className="space-y-1 text-muted-foreground">
          {roots.length ? roots.map((root) => <p key={root} className="truncate">{root}</p>) : <p>尚未选择媒体目录</p>}
        </div>
        <div className="flex gap-2">
          <Button size="sm" variant="outline" onClick={async () => {
            const selected = await open({ directory: true, multiple: true });
            if (!selected) return;
            patchSettings({ roots: Array.isArray(selected) ? selected : [selected] });
          }}>选择目录</Button>
          <Button size="sm" disabled={!roots.length || startMutation.isPending} onClick={() => startMutation.mutate()}>
            启动扫描
          </Button>
          <Button size="sm" variant="outline" disabled={!statusQuery.data?.running || stopMutation.isPending} onClick={() => stopMutation.mutate()}>
            停止
          </Button>
        </div>
        <label className="block text-muted-foreground">扫描周期（秒）
          <input className="ml-2 h-7 w-16 rounded border border-border bg-background px-1" type="number" min={5} value={pollIntervalSecs} onChange={(event) => patchSettings({ pollIntervalSecs: Math.max(5, Number(event.target.value) || 5) })} />
        </label>
      </section>

      <details className="rounded-md border border-border p-2">
        <summary className="cursor-pointer font-medium">智能匹配设置</summary>
        <div className="mt-2 space-y-2">
          <Field label="模型地址"><input value={modelBaseUrl} onChange={(e) => { setValidation(null); patchSettings({ modelBaseUrl: e.target.value }); }} placeholder="https://…/v1" /></Field>
          <Field label="模型 ID"><input value={modelId} onChange={(e) => { setValidation(null); patchSettings({ modelId: e.target.value }); }} placeholder="低成本 JSON 模型" /></Field>
          <Field label="模型 API Key（留空则不更新）"><input type="password" autoComplete="off" value={modelApiKey} onChange={(e) => { setValidation(null); setModelApiKey(e.target.value); }} placeholder={credentialStatusQuery.data?.modelApiKeySaved ? "已保存到此设备" : "输入后保存到此设备"} /></Field>
          <Field label="TMDb Read Access Token（留空则不更新）"><input type="password" autoComplete="off" value={tmdbAccessToken} onChange={(e) => { setValidation(null); setTmdbAccessToken(e.target.value); }} placeholder={credentialStatusQuery.data?.tmdbAccessTokenSaved ? "已保存到此设备" : "输入后保存到此设备"} /></Field>
          <div className="space-y-1 text-muted-foreground">
            <p>密钥保存在当前 Windows 用户的系统安全凭据中，不会写入 `.lumina`、项目文件或浏览器设置。</p>
            <div className="flex flex-wrap gap-1">
              <Button size="sm" disabled={(!modelApiKey && !tmdbAccessToken) || saveCredentialsMutation.isPending} onClick={() => saveCredentialsMutation.mutate()}>保存到此设备</Button>
              <Button size="sm" variant="outline" disabled={validateCredentialsMutation.isPending} onClick={() => validateCredentialsMutation.mutate()}>验证配置</Button>
              <Button size="sm" variant="outline" disabled={!credentialStatusQuery.data?.modelApiKeySaved || deleteCredentialMutation.isPending} onClick={() => deleteCredentialMutation.mutate("modelApiKey")}>删除模型密钥</Button>
              <Button size="sm" variant="outline" disabled={!credentialStatusQuery.data?.tmdbAccessTokenSaved || deleteCredentialMutation.isPending} onClick={() => deleteCredentialMutation.mutate("tmdbAccessToken")}>删除 TMDb Token</Button>
            </div>
            {validation ? <div className="space-y-1 rounded bg-muted/40 p-2"><ValidationItem label="模型服务" item={validation.model} /><ValidationItem label="TMDb" item={validation.tmdb} /></div> : null}
            <p>验证不会发送视频、字幕、笔记、文件名或绝对路径；模型验证会产生一次极小的 API 调用。</p>
          </div>
          <label className="flex gap-2 leading-relaxed text-muted-foreground"><input type="checkbox" checked={privacyAcknowledged} onChange={(e) => patchSettings({ privacyAcknowledged: e.target.checked })} />允许将文件名和相对目录名发送到所选模型服务；不会发送视频、字幕、笔记或绝对路径。</label>
        </div>
      </details>

      {error ? <p className="text-destructive">{error}</p> : null}
      <section className="min-h-0 space-y-2">
        <p className="font-medium">待匹配分组（{pendingQuery.data?.length ?? 0}）</p>
        {(pendingQuery.data ?? []).map((pending) => (
          <PendingGroupCard
            key={`${pending.root}:${pending.group.key}`}
            pending={pending}
            title={titles[pending.group.key] ?? pending.group.manualTitle ?? ""}
            preview={previews[pending.group.key]}
            disabled={!statusQuery.data?.running}
            onTitle={(title) => setTitles((state) => ({ ...state, [pending.group.key]: title }))}
            onSaveTitle={async (title) => {
              await setManualMediaTitle({ root: pending.root, groupKey: pending.group.key, title });
              await refresh();
            }}
            onPreview={async () => {
              const preview = await previewMediaMatch({ root: pending.root, groupKey: pending.group.key, config });
              setPreviews((state) => ({ ...state, [pending.group.key]: preview }));
            }}
            onApply={async (tmdbId, mediaType) => {
              await applyTmdbMediaMatch({ root: pending.root, groupKey: pending.group.key, tmdbId, mediaType, tmdb: config.tmdb });
              await refresh();
            }}
            onError={(err) => setError(errorMessage(err))}
          />
        ))}
      </section>
    </div>
  );
}

function ValidationItem({ label, item }: { label: string; item: CredentialValidationResult["model"] }) {
  return <p className={item.verified ? "text-emerald-500" : "text-destructive"}>{label}：{item.message}</p>;
}

function PendingGroupCard({ pending, title, preview, disabled, onTitle, onSaveTitle, onPreview, onApply, onError }: {
  pending: PendingMediaGroup; title: string; preview?: ResolverPreview; disabled: boolean;
  onTitle: (value: string) => void; onSaveTitle: (value: string) => Promise<void>;
  onPreview: () => Promise<void>; onApply: (id: number, type: "movie" | "tv") => Promise<void>;
  onError: (error: unknown) => void;
}) {
  return <div className="space-y-2 rounded-md border border-border p-2">
    <p className="font-medium">{pending.group.displayName}</p>
    <p className="text-muted-foreground">{pending.group.files.length} 个文件 · {pending.group.kind === "series" ? "剧集候选" : "电影候选"}</p>
    <div className="flex gap-1"><input className="h-7 min-w-0 flex-1 rounded border border-border bg-background px-2" value={title} placeholder="匹配不到时输入作品名" onChange={(e) => onTitle(e.target.value)} /><Button size="sm" variant="outline" disabled={!title.trim()} onClick={() => void onSaveTitle(title).catch(onError)}>保存标题</Button></div>
    <Button size="sm" disabled={disabled} onClick={() => void onPreview().catch(onError)}>智能识别</Button>
    {preview ? <div className="space-y-1 rounded bg-muted/40 p-2"><p>识别：{preview.intent.title} · {preview.intent.mediaType}</p>{preview.candidates.map((candidate) => <div key={candidate.tmdbId} className="flex items-center justify-between gap-2"><span className="min-w-0 truncate">{candidate.title}{candidate.year ? ` (${candidate.year})` : ""}</span><Button size="sm" variant="outline" onClick={() => void onApply(candidate.tmdbId, candidate.mediaType).catch(onError)}>确认</Button></div>)}</div> : null}
  </div>;
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return <label className="block space-y-1 text-muted-foreground"><span>{label}</span><span className="block [&_input]:h-7 [&_input]:w-full [&_input]:rounded [&_input]:border [&_input]:border-border [&_input]:bg-background [&_input]:px-2">{children}</span></label>;
}
