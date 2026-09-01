import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useMemo, useState } from "react";

import { Button } from "@/components/ui/button";
import { profilesHintFromStore } from "@/features/acp/defaultAgentProfiles";
import { useAcpProfilesStore } from "@/features/acp/acpProfilesStore";
import { errorMessage } from "@/lib/format";

import {
  applyTmdbMediaMatch,
  applyWikipediaPage,
  deleteMetadataCredential,
  discoverLibraryAgentModels,
  discoverLibraryModels,
  getLibraryStatus,
  getMetadataCredentialStatus,
  listLibraryGroups,
  listPendingMediaGroups,
  previewMediaMatch,
  previewWikipediaEnrichment,
  scanLibraryNow,
  saveMetadataCredentials,
  setManualMediaTitle,
  startLibraryWatch,
  stopLibraryWatch,
  validateMetadataCredentials,
  validateTmdbCredentials,
} from "../api";
import { useLibrarySettingsStore } from "../settingsStore";
import {
  buildAgentModelOptions,
  buildAgentReasoningOptions,
  isAgentResolverReady,
  isDirectResolverReady,
  mergeAgentDiscoverySettings,
} from "../resolverSettings";
import type {
  CredentialKind,
  CredentialValidationResult,
  CredentialValidationItem,
  AgentModelDiscoveryResult,
  MediaGroup,
  ModelDiscoveryResult,
  PendingMediaGroup,
  ResolverPreview,
  ResolverRunConfig,
  LibraryScanEvent,
  WikiEnrichmentCandidate,
  WikiEnrichmentPreview,
  WikiMatchMethod,
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
  const [wikiPreviews, setWikiPreviews] = useState<Record<string, WikiEnrichmentPreview>>({});
  const [wikiApplied, setWikiApplied] = useState<Record<string, boolean>>({});
  const [modelApiKey, setModelApiKey] = useState("");
  const [tmdbAccessToken, setTmdbAccessToken] = useState("");
  const [validation, setValidation] = useState<CredentialValidationResult | null>(null);
  const [tmdbValidation, setTmdbValidation] = useState<CredentialValidationItem | null>(null);
  const [modelConnection, setModelConnection] = useState<ModelDiscoveryResult | null>(null);
  const [agentConnection, setAgentConnection] = useState<AgentModelDiscoveryResult | null>(null);
  const [scanProgress, setScanProgress] = useState<LibraryScanEvent | null>(null);

  const statusQuery = useQuery({
    queryKey: ["library-status"],
    queryFn: getLibraryStatus,
    refetchInterval: (query) => query.state.data?.running ? 5_000 : false,
  });
  const watcherRefreshMs = statusQuery.data?.running
    ? Math.max(5, statusQuery.data.pollIntervalSecs) * 1_000
    : false;
  const pendingQuery = useQuery({
    queryKey: ["library-pending"],
    queryFn: listPendingMediaGroups,
    enabled: Boolean(statusQuery.data?.running),
    refetchInterval: watcherRefreshMs,
  });
  const primaryRoot = statusQuery.data?.roots[0] ?? roots[0] ?? "";
  const matchedQuery = useQuery({
    queryKey: ["library-groups", primaryRoot],
    queryFn: () => listLibraryGroups(primaryRoot),
    enabled: Boolean(statusQuery.data?.running && primaryRoot),
    refetchInterval: watcherRefreshMs,
  });
  const matchedGroups = useMemo(
    () =>
      (matchedQuery.data ?? []).filter(
        (group) => group.resolution.state === "matched",
      ),
    [matchedQuery.data],
  );
  const credentialStatusQuery = useQuery({
    queryKey: ["library-credential-status"],
    queryFn: getMetadataCredentialStatus,
  });
  const selectedAgentProfileId = acpProfiles.some((profile) => profile.id === agentProfileId)
    ? agentProfileId
    : activeAcpProfileId;

  useEffect(() => {
    if (agentProfileId.trim() || !activeAcpProfileId) return;
    patchSettings({ agentProfileId: activeAcpProfileId });
  }, [activeAcpProfileId, agentProfileId, patchSettings]);

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
  const persistPendingCredentials = async () => {
    const nextModelApiKey = modelApiKey.trim();
    const nextTmdbAccessToken = tmdbAccessToken.trim();
    if (!nextModelApiKey && !nextTmdbAccessToken) return;
    await saveMetadataCredentials({
      modelApiKey: nextModelApiKey || undefined,
      tmdbAccessToken: nextTmdbAccessToken || undefined,
    });
    setModelApiKey("");
    setTmdbAccessToken("");
    await queryClient.invalidateQueries({ queryKey: ["library-credential-status"] });
  };
  const startMutation = useMutation({
    mutationFn: () => startLibraryWatch({ roots, pollIntervalSecs }, setScanProgress),
    onSuccess: () => void refresh(),
    onError: (err) => setError(errorMessage(err)),
  });
  const stopMutation = useMutation({
    mutationFn: stopLibraryWatch,
    onSuccess: () => void refresh(),
    onError: (err) => setError(errorMessage(err)),
  });
  const scanNowMutation = useMutation({
    mutationFn: scanLibraryNow,
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
    mutationFn: async () => {
      await persistPendingCredentials();
      return validateMetadataCredentials({
        provider: config.provider,
        tmdb: config.tmdb,
      });
    },
    onSuccess: (result) => setValidation(result),
    onError: (err) => setError(errorMessage(err)),
  });
  const validateTmdbMutation = useMutation({
    mutationFn: async () => {
      await persistPendingCredentials();
      return validateTmdbCredentials(config.tmdb);
    },
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
      const saved = useLibrarySettingsStore.getState();
      const patch = mergeAgentDiscoverySettings(
        {
          agentModelId: saved.agentModelId,
          agentReasoningEffort: saved.agentReasoningEffort,
        },
        result,
      );
      if (Object.keys(patch).length > 0) patchSettings(patch);
    },
    onError: (err) => setError(errorMessage(err)),
  });
  const directModelReady = isDirectResolverReady({
    modelId,
    modelBaseUrl,
    modelApiKeySaved: Boolean(credentialStatusQuery.data?.modelApiKeySaved),
    pendingApiKey: modelApiKey,
  });
  const agentModelReady = isAgentResolverReady(agentModelId);
  const resolverReady = resolverProvider === "directApi" ? directModelReady : agentModelReady;
  const selectedModelOption = modelConnection?.models.includes(modelId) ? modelId : "__manual__";
  const agentModelOptions = buildAgentModelOptions(agentConnection, agentModelId);
  const agentReasoningOptions = buildAgentReasoningOptions(
    agentConnection,
    agentReasoningEffort,
  );
  const showAgentModelPicker = Boolean(
    agentConnection?.connected || agentModelId.trim(),
  );
  const showDirectModelPicker = Boolean(
    modelConnection?.connected ||
      (modelId.trim() &&
        modelBaseUrl.trim() &&
        credentialStatusQuery.data?.modelApiKeySaved),
  );
  const tmdbTokenSaved = Boolean(credentialStatusQuery.data?.tmdbAccessTokenSaved);

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
          {statusQuery.data?.lastScanAtMs ? <p>上次成功扫描：{formatScanTime(statusQuery.data.lastScanAtMs)} · 已索引 {statusQuery.data.indexedFiles} 个文件</p> : null}
        </div>
        <div className="flex gap-2">
          <Button size="sm" variant="outline" onClick={async () => {
            const selected = await open({ directory: true, multiple: true });
            if (!selected) return;
            patchSettings({ roots: Array.isArray(selected) ? selected : [selected] });
          }}>选择目录</Button>
          <Button size="sm" disabled={!roots.length || startMutation.isPending} onClick={() => { setError(null); setScanProgress({ type: "Started", payload: { rootCount: roots.length } }); startMutation.mutate(); }}>
            {startMutation.isPending ? "正在扫描" : "启动扫描"}
          </Button>
          <Button size="sm" variant="outline" disabled={!statusQuery.data?.running || stopMutation.isPending} onClick={() => stopMutation.mutate()}>
            停止
          </Button>
        </div>
        <label className="block text-muted-foreground">扫描周期（秒）
          <input className="ml-2 h-7 w-16 rounded border border-border bg-background px-1" type="number" min={5} value={pollIntervalSecs} onChange={(event) => patchSettings({ pollIntervalSecs: Math.max(5, Number(event.target.value) || 5) })} />
        </label>
        {scanProgress ? <ScanProgressCard event={scanProgress} busy={startMutation.isPending} /> : null}
        <p className="text-muted-foreground">扫描和本地索引不需要连接 Agent 或配置 TMDb；智能匹配时才会使用它们。</p>
        {statusQuery.data?.lastScanError ? <div className="space-y-2 rounded-md border border-destructive/50 bg-destructive/10 p-2 text-destructive" role="alert">
          <p>上一次扫描失败：{statusQuery.data.lastScanError.message}</p>
          <p className="text-muted-foreground">守护服务会在下个扫描周期自动重试；你也可以立即重试。</p>
          <Button size="sm" variant="outline" disabled={scanNowMutation.isPending || !statusQuery.data.roots.length} onClick={() => scanNowMutation.mutate()}>{scanNowMutation.isPending ? "重试中" : "立即重试"}</Button>
        </div> : null}
      </section>

      <details className="rounded-md border border-border p-2">
        <summary className="cursor-pointer font-medium">智能匹配设置</summary>
        <div className="mt-2 space-y-2">
          <Field label="智能匹配来源"><select className="h-7 w-full rounded border border-border bg-background px-2" value={resolverProvider} onChange={(e) => { setValidation(null); setModelConnection(null); setAgentConnection(null); patchSettings({ resolverProvider: e.target.value as "acpAgent" | "directApi" }); }}><option value="directApi">独立模型服务（推荐）</option><option value="acpAgent">复用 Lumina Agent（高级）</option></select></Field>
          {resolverProvider === "acpAgent" ? <>
            <Field label="用于智能匹配的 Agent"><select className="h-7 w-full rounded border border-border bg-background px-2" value={selectedAgentProfileId} onChange={(e) => { setValidation(null); setAgentConnection(null); patchSettings({ agentProfileId: e.target.value, agentModelId: "", agentReasoningEffort: "" }); }}>{acpProfiles.map((profile) => <option key={profile.id} value={profile.id}>{profile.name}</option>)}</select></Field>
            <div className="space-y-1"><Button size="sm" variant="outline" disabled={!selectedAgentProfileId || discoverAgentModelsMutation.isPending} onClick={() => discoverAgentModelsMutation.mutate()}>{discoverAgentModelsMutation.isPending ? "连接中" : agentModelId ? "刷新 Agent 模型列表" : "连接 Agent 并获取模型"}</Button>{agentConnection ? <p className={agentConnection.connected ? "text-emerald-500" : "text-destructive"}>{agentConnection.message}</p> : agentModelId ? <p className="text-muted-foreground">已保存媒体匹配模型：{agentModelId}{agentReasoningEffort ? ` · ${agentReasoningEffort}` : ""}。重启后可直接智能匹配；仅在更换模型时需要刷新列表。</p> : <p className="text-muted-foreground">先连接 Agent，再选择本次媒体匹配使用的模型；不会复用或改写聊天会话。</p>}</div>
            {showAgentModelPicker && agentModelOptions.length ? <><Field label="用于媒体匹配的模型"><select className="h-7 w-full rounded border border-border bg-background px-2" value={agentModelId} onChange={(e) => { setValidation(null); patchSettings({ agentModelId: e.target.value }); }}><option value="">请选择模型</option>{agentModelOptions.map((option) => <option key={option.value} value={option.value}>{option.name}</option>)}</select></Field><p className="text-muted-foreground">文件名解析是轻量任务：优先选择账户可用的低成本/mini 模型，而非最长任务或旗舰模型。</p>{agentReasoningOptions.length ? <Field label="推理强度"><select className="h-7 w-full rounded border border-border bg-background px-2" value={agentReasoningEffort} onChange={(e) => { setValidation(null); patchSettings({ agentReasoningEffort: e.target.value }); }}><option value="">使用模型默认值</option>{agentReasoningOptions.map((option) => <option key={option.value} value={option.value}>{option.name}</option>)}</select></Field> : null}<p className="text-muted-foreground">建议设为 low 或 minimal（若该模型提供），以降低文件名匹配的延迟与成本。</p></> : null}
            <p className="text-muted-foreground">每次识别创建独立短会话，发出请求前会应用这里选定的模型与推理强度；不读取或写入 AI 对话历史，也不允许工具、文件系统或终端访问。</p>
          </> : <>
            <Field label="模型服务地址"><input value={modelBaseUrl} onChange={(e) => { setValidation(null); setModelConnection(null); patchSettings({ modelBaseUrl: e.target.value }); }} placeholder="https://…/v1" /></Field>
            <Field label="模型 API Key（留空则沿用已保存密钥）"><input type="password" autoComplete="off" value={modelApiKey} onChange={(e) => { setValidation(null); setModelConnection(null); setModelApiKey(e.target.value); }} placeholder={credentialStatusQuery.data?.modelApiKeySaved ? "已保存到此设备" : "输入后保存到此设备"} /></Field>
            <div className="space-y-1"><Button size="sm" variant="outline" disabled={!modelBaseUrl.trim() || (!modelApiKey.trim() && !credentialStatusQuery.data?.modelApiKeySaved) || discoverModelsMutation.isPending} onClick={() => discoverModelsMutation.mutate()}>{modelApiKey.trim() ? "保存并连接" : "连接并获取模型"}</Button>{modelConnection ? <p className={modelConnection.connected ? "text-emerald-500" : "text-destructive"}>{modelConnection.message}</p> : <p className="text-muted-foreground">连接成功后再选择模型；模型服务和聊天 Agent 的配置彼此独立。</p>}</div>
            {showDirectModelPicker ? <Field label="用于媒体匹配的模型">{modelConnection?.connected && modelConnection.models.length ? <select className="h-7 w-full rounded border border-border bg-background px-2" value={selectedModelOption} onChange={(e) => { setValidation(null); patchSettings({ modelId: e.target.value === "__manual__" ? "" : e.target.value }); }}><option value="">请选择模型</option>{modelConnection.models.map((model) => <option key={model} value={model}>{model}</option>)}<option value="__manual__">手动输入模型 ID</option></select> : null}{(!modelConnection?.connected || !modelConnection.models.length || selectedModelOption === "__manual__") ? <input value={modelId} onChange={(e) => { setValidation(null); patchSettings({ modelId: e.target.value }); }} placeholder="输入兼容服务的模型 ID" /> : null}</Field> : null}
          </>}
          <Field label={tmdbTokenSaved && !tmdbAccessToken.trim() ? "TMDb Read Access Token（已保存到此设备）" : "TMDb Read Access Token（留空则不更新）"}><input type="password" autoComplete="off" value={tmdbAccessToken} onChange={(e) => { setValidation(null); setTmdbValidation(null); setTmdbAccessToken(e.target.value); }} placeholder={tmdbTokenSaved ? "已保存，无需重新输入；仅在更换 Token 时填写" : "输入后保存到此设备"} /></Field>
          <div className="space-y-1 text-muted-foreground">
            <p>密钥保存在当前 Windows 用户的系统安全凭据中，不会写入 `.lumina`、项目文件或浏览器设置。</p>
            <p>{tmdbAccessToken.trim() ? "已输入新 TMDb Token，点击“保存到此设备”后才会长期保存。" : tmdbTokenSaved ? "TMDb Token 已安全保存到此设备，可直接用于 TMDb 匹配与维基补充，无需重新输入。" : "尚未保存 TMDb Token。"}</p>
            <div className="flex flex-wrap gap-1">
              <Button size="sm" disabled={(!modelApiKey && !tmdbAccessToken) || saveCredentialsMutation.isPending} onClick={() => saveCredentialsMutation.mutate()}>保存到此设备</Button>
              <Button size="sm" variant="outline" disabled={validateCredentialsMutation.isPending} onClick={() => validateCredentialsMutation.mutate()}>{modelApiKey.trim() || tmdbAccessToken.trim() ? "保存并验证全部配置" : "验证全部配置"}</Button>
              <Button size="sm" variant="outline" disabled={(!tmdbAccessToken.trim() && !credentialStatusQuery.data?.tmdbAccessTokenSaved) || validateTmdbMutation.isPending} onClick={() => validateTmdbMutation.mutate()}>{tmdbAccessToken.trim() ? "保存并验证 TMDb" : "验证 TMDb Token"}</Button>
              {resolverProvider === "directApi" ? <Button size="sm" variant="outline" disabled={!credentialStatusQuery.data?.modelApiKeySaved || deleteCredentialMutation.isPending} onClick={() => deleteCredentialMutation.mutate("modelApiKey")}>删除模型密钥</Button> : null}
              <Button size="sm" variant="outline" disabled={!credentialStatusQuery.data?.tmdbAccessTokenSaved || deleteCredentialMutation.isPending} onClick={() => deleteCredentialMutation.mutate("tmdbAccessToken")}>删除 TMDb Token</Button>
            </div>
            {validation ? <div className="space-y-1 rounded bg-muted/40 p-2"><ValidationItem label={resolverProvider === "acpAgent" ? "Agent" : "模型服务"} item={validation.model} /><ValidationItem label="TMDb" item={validation.tmdb} /></div> : null}
            {tmdbValidation ? <div className="rounded bg-muted/40 p-2"><ValidationItem label="TMDb" item={tmdbValidation} /></div> : null}
            <p>验证不会发送视频、字幕、笔记、文件名或绝对路径；“验证全部配置”会产生一次极小的模型或 Agent 调用，TMDb 单独验证不会。</p>
          </div>
          <label className="flex gap-2 leading-relaxed text-muted-foreground"><input type="checkbox" checked={privacyAcknowledged} onChange={(e) => patchSettings({ privacyAcknowledged: e.target.checked })} />允许将文件名和相对目录名发送到所选解析器；不会发送视频、字幕、笔记或绝对路径。</label>
        </div>
      </details>

      {error ? <p className="text-destructive" role="alert">{error}</p> : null}
      {matchedGroups.length > 0 ? (
        <section className="min-h-0 space-y-2">
          <p className="font-medium">已匹配分组（{matchedGroups.length}）</p>
          <p className="text-muted-foreground">
            完成 TMDb 匹配后，可补充英文维基百科摘要（CC BY-SA）；需已保存 TMDb Token 且联网。
          </p>
          {matchedGroups.map((group) => (
            <MatchedGroupCard
              key={`${primaryRoot}:${group.key}`}
              root={primaryRoot}
              group={group}
              preview={wikiPreviews[group.key]}
              applied={wikiApplied[group.key]}
              tmdb={config.tmdb}
              disabled={!credentialStatusQuery.data?.tmdbAccessTokenSaved}
              onPreview={async () => {
                const preview = await previewWikipediaEnrichment({
                  root: primaryRoot,
                  groupKey: group.key,
                  tmdb: config.tmdb,
                });
                setWikiPreviews((state) => ({ ...state, [group.key]: preview }));
                return preview;
              }}
              onApply={async (candidate, matchMethod, candidatesConsidered) => {
                await applyWikipediaPage({
                  root: primaryRoot,
                  groupKey: group.key,
                  candidate,
                  matchMethod,
                  candidatesConsidered,
                });
                setWikiApplied((state) => ({ ...state, [group.key]: true }));
                const preview = await previewWikipediaEnrichment({
                  root: primaryRoot,
                  groupKey: group.key,
                  tmdb: config.tmdb,
                });
                setWikiPreviews((state) => ({ ...state, [group.key]: preview }));
              }}
              onError={(err) => setError(errorMessage(err))}
            />
          ))}
        </section>
      ) : null}
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

function formatScanTime(timestamp: number): string {
  const date = new Date(timestamp);
  if (Number.isNaN(date.getTime())) return "时间未知";
  return date.toLocaleString("zh-CN", { hour12: false });
}

function ScanProgressCard({ event, busy }: { event: LibraryScanEvent; busy: boolean }) {
  if (event.type === "Failed") return <div className="rounded-md border border-destructive/50 bg-destructive/10 p-2 text-destructive" role="alert">首次扫描失败：{event.payload.message}</div>;
  if (event.type === "Finished") return <div className="rounded-md border border-emerald-500/40 bg-emerald-500/10 p-2 text-emerald-500">首次扫描完成：已索引 {event.payload.indexedFiles} 个文件，发现 {event.payload.pendingGroups} 个待匹配分组。</div>;
  const rootCount = event.payload.rootCount;
  const rootsCompleted = event.type === "Progress" ? event.payload.rootsCompleted : 0;
  const indexedFiles = event.type === "Progress" ? event.payload.indexedFiles : 0;
  return <div className="rounded-md border border-border bg-muted/40 p-2 text-muted-foreground" role="status"><p>{busy ? "正在首次扫描媒体目录…" : "正在准备扫描…"}</p><p>目录进度：{rootsCompleted} / {rootCount} · 已发现 {indexedFiles} 个视频文件</p><div className="mt-1 h-1 overflow-hidden rounded bg-border"><div className="h-full bg-primary transition-all" style={{ width: `${rootCount ? Math.max(8, rootsCompleted / rootCount * 100) : 8}%` }} /></div></div>;
}

function ValidationItem({ label, item }: { label: string; item: CredentialValidationResult["model"] }) {
  return <p className={item.verified ? "text-emerald-500" : "text-destructive"}>{label}：{item.message}</p>;
}

function wikiCandidatesConsidered(preview: WikiEnrichmentPreview): number {
  return preview.searchCandidates.length + (preview.wikidataCandidate ? 1 : 0);
}

function matchMethodForCandidate(
  candidate: WikiEnrichmentCandidate,
  userSelected: boolean,
): WikiMatchMethod {
  if (userSelected) return "userSelected";
  return candidate.source === "wikidata" ? "wikidata" : "search";
}

function MatchedGroupCard({
  group,
  preview,
  applied,
  disabled,
  onPreview,
  onApply,
  onError,
}: {
  root: string;
  group: MediaGroup;
  preview?: WikiEnrichmentPreview;
  applied?: boolean;
  tmdb: ResolverRunConfig["tmdb"];
  disabled: boolean;
  onPreview: () => Promise<WikiEnrichmentPreview>;
  onApply: (
    candidate: WikiEnrichmentCandidate,
    matchMethod: WikiMatchMethod,
    candidatesConsidered: number,
  ) => Promise<void>;
  onError: (error: unknown) => void;
}) {
  const candidates = [
    ...(preview?.wikidataCandidate ? [preview.wikidataCandidate] : []),
    ...(preview?.searchCandidates ?? []),
  ].filter(
    (candidate, index, list) =>
      list.findIndex(
        (item) =>
          item.pageTitle === candidate.pageTitle &&
          item.pageLang === candidate.pageLang,
      ) === index,
  );

  return (
    <div className="space-y-2 rounded-md border border-border p-2">
      <p className="font-medium">{group.displayName}</p>
      <p className="text-muted-foreground">
        {group.files.length} 个文件 · TMDb 已匹配
      </p>
      <Button
        size="sm"
        variant="outline"
        disabled={disabled}
        onClick={() => {
          void onPreview()
            .then((nextPreview) => {
              if (
                !nextPreview.needsUserPick &&
                nextPreview.recommended &&
                !nextPreview.conflict
              ) {
                return onApply(
                  nextPreview.recommended,
                  matchMethodForCandidate(nextPreview.recommended, false),
                  wikiCandidatesConsidered(nextPreview),
                );
              }
            })
            .catch(onError);
        }}
      >
        补充维基（英文）
      </Button>
      {preview ? (
        <div className="space-y-1 rounded bg-muted/40 p-2 text-xs">
          {preview.conflict ? (
            <p className="text-amber-600 dark:text-amber-400">
              Wikidata 与搜索结果不一致，请选择正确页面。
            </p>
          ) : null}
          {applied && preview?.recommended ? (
            <p className="text-emerald-600 dark:text-emerald-400">
              已写入：{preview.recommended.pageTitle}
            </p>
          ) : null}
          {candidates.map((candidate) => (
            <div
              key={`${candidate.pageLang}:${candidate.pageTitle}`}
              className="flex items-start justify-between gap-2"
            >
              <div className="min-w-0">
                <p className="truncate font-medium">{candidate.pageTitle}</p>
                {candidate.extract ? (
                  <p className="line-clamp-2 text-muted-foreground">
                    {candidate.extract}
                  </p>
                ) : null}
              </div>
              {preview.needsUserPick || preview.conflict ? (
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() =>
                    void onApply(
                      candidate,
                      matchMethodForCandidate(candidate, true),
                      wikiCandidatesConsidered(preview),
                    ).catch(onError)
                  }
                >
                  选用
                </Button>
              ) : null}
            </div>
          ))}
          {candidates.length === 0 ? (
            <p className="text-muted-foreground">未找到英文维基页面。</p>
          ) : null}
        </div>
      ) : null}
    </div>
  );
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
