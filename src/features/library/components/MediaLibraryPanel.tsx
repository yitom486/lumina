import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { useMemo, useState } from "react";

import { Button } from "@/components/ui/button";
import { profilesHintFromStore } from "@/features/acp/defaultAgentProfiles";
import { useAcpProfilesStore } from "@/features/acp/acpProfilesStore";
import { errorMessage } from "@/lib/format";

import {
  applyTmdbMediaMatch,
  deleteMetadataCredential,
  discoverLibraryAgentModels,
  discoverLibraryModels,
  getLibraryStatus,
  getMetadataCredentialStatus,
  listPendingMediaGroups,
  previewMediaMatch,
  saveMetadataCredentials,
  setManualMediaTitle,
  startLibraryWatch,
  stopLibraryWatch,
  validateMetadataCredentials,
  validateTmdbCredentials,
} from "../api";
import { useLibrarySettingsStore } from "../settingsStore";
import type {
  CredentialKind,
  CredentialValidationResult,
  CredentialValidationItem,
  AgentModelDiscoveryResult,
  ModelDiscoveryResult,
  PendingMediaGroup,
  ResolverPreview,
  ResolverRunConfig,
} from "../types";

export function MediaLibraryPanel() {
  const queryClient = useQueryClient();
  const roots = useLibrarySettingsStore((state) => state.roots);
  const pollIntervalSecs = useLibrarySettingsStore((state) => state.pollIntervalSecs);
  const privacyAcknowledged = useLibrarySettingsStore((state) => state.privacyAcknowledged);
  const resolverProvider = useLibrarySettingsStore((state) => state.resolverProvider);
  const agentProfileId = useLibrarySettingsStore((state) => state.agentProfileId);
  const agentModelId = useLibrarySettingsStore((state) => state.agentModelId);
  const agentReasoningEffort = useLibrarySettingsStore((state) => state.agentReasoningEffort);
  const modelBaseUrl = useLibrarySettingsStore((state) => state.modelBaseUrl);
  const modelId = useLibrarySettingsStore((state) => state.modelId);
  const tmdbLanguage = useLibrarySettingsStore((state) => state.tmdbLanguage);
  const patchSettings = useLibrarySettingsStore((state) => state.patchSettings);
  const activeAcpProfileId = useAcpProfilesStore((state) => state.activeProfileId);
  const acpProfiles = useAcpProfilesStore((state) => state.profiles);
  const [error, setError] = useState<string | null>(null);
  const [titles, setTitles] = useState<Record<string, string>>({});
  const [previews, setPreviews] = useState<Record<string, ResolverPreview>>({});
  const [modelApiKey, setModelApiKey] = useState("");
  const [tmdbAccessToken, setTmdbAccessToken] = useState("");
  const [validation, setValidation] = useState<CredentialValidationResult | null>(null);
  const [tmdbValidation, setTmdbValidation] = useState<CredentialValidationItem | null>(null);
  const [modelConnection, setModelConnection] = useState<ModelDiscoveryResult | null>(null);
  const [agentConnection, setAgentConnection] = useState<AgentModelDiscoveryResult | null>(null);

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
  const selectedAgentProfileId = acpProfiles.some((profile) => profile.id === agentProfileId)
    ? agentProfileId
    : activeAcpProfileId;
  const config = useMemo<ResolverRunConfig>(
    () => {
      const profiles = profilesHintFromStore(activeAcpProfileId, acpProfiles);
      const provider = resolverProvider === "acpAgent"
          ? {
            kind: "acpAgent" as const,
            profileId: selectedAgentProfileId,
            profiles,
            modelId: agentModelId || undefined,
            reasoningEffort: agentReasoningEffort || undefined,
          }
        : {
            kind: "directApi" as const,
            // Environment variables remain a development/CI fallback only.
            model: { baseUrl: modelBaseUrl, modelId, apiKeyEnv: "LUMINA_METADATA_MODEL_API_KEY" },
          };
      return {
        privacyAcknowledged,
        provider,
        tmdb: { accessTokenEnv: "LUMINA_TMDB_ACCESS_TOKEN", language: tmdbLanguage },
      };
    },
    [activeAcpProfileId, acpProfiles, agentModelId, agentReasoningEffort, modelBaseUrl, modelId, privacyAcknowledged, resolverProvider, selectedAgentProfileId, tmdbLanguage],
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
      provider: config.provider,
      tmdb: config.tmdb,
    }),
    onSuccess: (result) => setValidation(result),
    onError: (err) => setError(errorMessage(err)),
  });
  const validateTmdbMutation = useMutation({
    mutationFn: () => validateTmdbCredentials(config.tmdb),
    onSuccess: setTmdbValidation,
    onError: (err) => setError(errorMessage(err)),
  });
  const discoverModelsMutation = useMutation({
    mutationFn: async () => {
      if (modelApiKey.trim()) {
        await saveMetadataCredentials({ modelApiKey: modelApiKey.trim() });
        setModelApiKey("");
        await queryClient.invalidateQueries({ queryKey: ["library-credential-status"] });
      }
      return discoverLibraryModels({
        baseUrl: modelBaseUrl,
        apiKeyEnv: "LUMINA_METADATA_MODEL_API_KEY",
      });
    },
    onSuccess: setModelConnection,
    onError: (err) => setError(errorMessage(err)),
  });
  const discoverAgentModelsMutation = useMutation({
    mutationFn: () => discoverLibraryAgentModels({
      profileId: selectedAgentProfileId,
      profiles: profilesHintFromStore(activeAcpProfileId, acpProfiles),
    }),
    onSuccess: (result) => {
      setAgentConnection(result);
      if (!result.connected) return;
      patchSettings({
        agentModelId: result.options.currentModelId ?? "",
        agentReasoningEffort: result.options.currentReasoningEffort ?? "",
      });
    },
    onError: (err) => setError(errorMessage(err)),
  });
  const directModelReady = Boolean(modelConnection?.connected && modelId.trim());
  const agentModelReady = Boolean(agentConnection?.connected && (!agentConnection.options.models.length || agentModelId));
  const resolverReady = resolverProvider === "directApi" ? directModelReady : agentModelReady;
  const selectedModelOption = modelConnection?.models.includes(modelId) ? modelId : "__manual__";

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
          <Field label="智能匹配来源"><select className="h-7 w-full rounded border border-border bg-background px-2" value={resolverProvider} onChange={(e) => { setValidation(null); setModelConnection(null); setAgentConnection(null); patchSettings({ resolverProvider: e.target.value as "acpAgent" | "directApi" }); }}><option value="directApi">独立模型服务（推荐）</option><option value="acpAgent">复用 Lumina Agent（高级）</option></select></Field>
          {resolverProvider === "acpAgent" ? <>
            <Field label="用于智能匹配的 Agent"><select className="h-7 w-full rounded border border-border bg-background px-2" value={selectedAgentProfileId} onChange={(e) => { setValidation(null); setAgentConnection(null); patchSettings({ agentProfileId: e.target.value, agentModelId: "", agentReasoningEffort: "" }); }}>{acpProfiles.map((profile) => <option key={profile.id} value={profile.id}>{profile.name}</option>)}</select></Field>
            <div className="space-y-1"><Button size="sm" variant="outline" disabled={!selectedAgentProfileId || discoverAgentModelsMutation.isPending} onClick={() => discoverAgentModelsMutation.mutate()}>{discoverAgentModelsMutation.isPending ? "连接中" : "连接 Agent 并获取模型"}</Button>{agentConnection ? <p className={agentConnection.connected ? "text-emerald-500" : "text-destructive"}>{agentConnection.message}</p> : <p className="text-muted-foreground">先连接 Agent，再选择本次媒体匹配使用的模型；不会复用或改写聊天会话。</p>}</div>
            {agentConnection?.connected && agentConnection.options.models.length ? <><Field label="用于媒体匹配的模型"><select className="h-7 w-full rounded border border-border bg-background px-2" value={agentModelId} onChange={(e) => { setValidation(null); patchSettings({ agentModelId: e.target.value }); }}><option value="">请选择模型</option>{agentConnection.options.models.map((option) => <option key={option.value} value={option.value}>{option.name}</option>)}</select></Field><p className="text-muted-foreground">文件名解析是轻量任务：优先选择账户可用的低成本/mini 模型，而非最长任务或旗舰模型。</p>{agentConnection.options.reasoningEfforts.length ? <Field label="推理强度"><select className="h-7 w-full rounded border border-border bg-background px-2" value={agentReasoningEffort} onChange={(e) => { setValidation(null); patchSettings({ agentReasoningEffort: e.target.value }); }}><option value="">使用模型默认值</option>{agentConnection.options.reasoningEfforts.map((option) => <option key={option.value} value={option.value}>{option.name}</option>)}</select></Field> : null}<p className="text-muted-foreground">建议设为 low 或 minimal（若该模型提供），以降低文件名匹配的延迟与成本。</p></> : null}
            <p className="text-muted-foreground">每次识别创建独立短会话，发出请求前会应用这里选定的模型与推理强度；不读取或写入 AI 对话历史，也不允许工具、文件系统或终端访问。</p>
          </> : <>
            <Field label="模型服务地址"><input value={modelBaseUrl} onChange={(e) => { setValidation(null); setModelConnection(null); patchSettings({ modelBaseUrl: e.target.value }); }} placeholder="https://…/v1" /></Field>
            <Field label="模型 API Key（留空则沿用已保存密钥）"><input type="password" autoComplete="off" value={modelApiKey} onChange={(e) => { setValidation(null); setModelConnection(null); setModelApiKey(e.target.value); }} placeholder={credentialStatusQuery.data?.modelApiKeySaved ? "已保存到此设备" : "输入后保存到此设备"} /></Field>
            <div className="space-y-1"><Button size="sm" variant="outline" disabled={!modelBaseUrl.trim() || (!modelApiKey.trim() && !credentialStatusQuery.data?.modelApiKeySaved) || discoverModelsMutation.isPending} onClick={() => discoverModelsMutation.mutate()}>{modelApiKey.trim() ? "保存并连接" : "连接并获取模型"}</Button>{modelConnection ? <p className={modelConnection.connected ? "text-emerald-500" : "text-destructive"}>{modelConnection.message}</p> : <p className="text-muted-foreground">连接成功后再选择模型；模型服务和聊天 Agent 的配置彼此独立。</p>}</div>
            {modelConnection?.connected ? <Field label="用于媒体匹配的模型">{modelConnection.models.length ? <select className="h-7 w-full rounded border border-border bg-background px-2" value={selectedModelOption} onChange={(e) => { setValidation(null); patchSettings({ modelId: e.target.value === "__manual__" ? "" : e.target.value }); }}><option value="">请选择模型</option>{modelConnection.models.map((model) => <option key={model} value={model}>{model}</option>)}<option value="__manual__">手动输入模型 ID</option></select> : null}{(!modelConnection.models.length || selectedModelOption === "__manual__") ? <input value={modelId} onChange={(e) => { setValidation(null); patchSettings({ modelId: e.target.value }); }} placeholder="输入兼容服务的模型 ID" /> : null}</Field> : null}
          </>}
          <Field label="TMDb Read Access Token（留空则不更新）"><input type="password" autoComplete="off" value={tmdbAccessToken} onChange={(e) => { setValidation(null); setTmdbValidation(null); setTmdbAccessToken(e.target.value); }} placeholder={credentialStatusQuery.data?.tmdbAccessTokenSaved ? "已保存到此设备" : "输入后保存到此设备"} /></Field>
          <div className="space-y-1 text-muted-foreground">
            <p>密钥保存在当前 Windows 用户的系统安全凭据中，不会写入 `.lumina`、项目文件或浏览器设置。</p>
            <div className="flex flex-wrap gap-1">
              <Button size="sm" disabled={(!modelApiKey && !tmdbAccessToken) || saveCredentialsMutation.isPending} onClick={() => saveCredentialsMutation.mutate()}>保存到此设备</Button>
              <Button size="sm" variant="outline" disabled={validateCredentialsMutation.isPending} onClick={() => validateCredentialsMutation.mutate()}>验证全部配置</Button>
              <Button size="sm" variant="outline" disabled={!credentialStatusQuery.data?.tmdbAccessTokenSaved || validateTmdbMutation.isPending} onClick={() => validateTmdbMutation.mutate()}>验证 TMDb Token</Button>
              {resolverProvider === "directApi" ? <Button size="sm" variant="outline" disabled={!credentialStatusQuery.data?.modelApiKeySaved || deleteCredentialMutation.isPending} onClick={() => deleteCredentialMutation.mutate("modelApiKey")}>删除模型密钥</Button> : null}
              <Button size="sm" variant="outline" disabled={!credentialStatusQuery.data?.tmdbAccessTokenSaved || deleteCredentialMutation.isPending} onClick={() => deleteCredentialMutation.mutate("tmdbAccessToken")}>删除 TMDb Token</Button>
            </div>
            {validation ? <div className="space-y-1 rounded bg-muted/40 p-2"><ValidationItem label={resolverProvider === "acpAgent" ? "Agent" : "模型服务"} item={validation.model} /><ValidationItem label="TMDb" item={validation.tmdb} /></div> : null}
            {tmdbValidation ? <div className="rounded bg-muted/40 p-2"><ValidationItem label="TMDb" item={tmdbValidation} /></div> : null}
            <p>验证不会发送视频、字幕、笔记、文件名或绝对路径；验证会产生一次极小的模型或 Agent 调用。</p>
          </div>
          <label className="flex gap-2 leading-relaxed text-muted-foreground"><input type="checkbox" checked={privacyAcknowledged} onChange={(e) => patchSettings({ privacyAcknowledged: e.target.checked })} />允许将文件名和相对目录名发送到所选解析器；不会发送视频、字幕、笔记或绝对路径。</label>
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
            disabled={!statusQuery.data?.running || !resolverReady}
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
