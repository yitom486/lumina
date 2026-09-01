import { useEffect, useMemo, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";

import { Button } from "@/components/ui/button";
import { errorMessage } from "@/lib/format";

import { useAcpProfilesStore } from "../acpProfilesStore";
import { useAcpSettingsStore } from "../acpSettingsStore";
import { acpSetSessionModel, discoverAcpModels } from "../api";
import { profilesHintFromStore } from "../defaultAgentProfiles";
import {
  buildModelOptions,
  buildReasoningOptions,
  mergeSessionModelDefaults,
} from "../modelOptions";
import type {
  AcpSessionModelOptions,
  AcpStatus,
  AgentProfileStatus,
  PermissionMode,
  ThinkingLevel,
} from "../types";
import { ChatColumn } from "./ChatShell";

type Props = {
  status: AcpStatus | undefined;
  busy?: boolean;
  sessionConnected?: boolean;
};

const AGENT_MODES = [
  { id: "default", label: "默认" },
  { id: "plan", label: "计划优先" },
  { id: "fast", label: "快速" },
] as const;

const selectClassName =
  "h-8 w-full rounded-md border border-border bg-background px-2 text-xs";

/** Agent profile + client prefs — collapsed at bottom of chat column. */
export function AgentSettingsPanel({
  status,
  busy,
  sessionConnected,
}: Props) {
  const queryClient = useQueryClient();
  const [open, setOpen] = useState(false);
  const [customCommand, setCustomCommand] = useState("");
  const [discoveredOptions, setDiscoveredOptions] =
    useState<AcpSessionModelOptions | null>(null);
  const [discoverBusy, setDiscoverBusy] = useState(false);
  const [discoverError, setDiscoverError] = useState<string | null>(null);

  const activeProfileId = useAcpProfilesStore((s) => s.activeProfileId);
  const profiles = useAcpProfilesStore((s) => s.profiles);
  const setActiveProfileId = useAcpProfilesStore((s) => s.setActiveProfileId);
  const upsertProfile = useAcpProfilesStore((s) => s.upsertProfile);
  const permissionMode = useAcpSettingsStore((s) => s.permissionMode);
  const thinkingLevel = useAcpSettingsStore((s) => s.thinkingLevel);
  const agentMode = useAcpSettingsStore((s) => s.agentMode);
  const modelId = useAcpSettingsStore((s) => s.modelId ?? "");
  const reasoningEffort = useAcpSettingsStore((s) => s.reasoningEffort ?? "");
  const patchSettings = useAcpSettingsStore((s) => s.patchSettings);

  const customProfileCommand = status?.profiles.find(
    (profile) => profile.id === "custom",
  )?.command;

  useEffect(() => {
    if (customProfileCommand !== undefined) {
      setCustomCommand(customProfileCommand);
    }
  }, [customProfileCommand]);

  const profileList = status?.profiles ?? [];
  const active = profileList.find((profile) => profile.id === activeProfileId);
  const showResponsesNote =
    active?.kind === "Codex" || activeProfileId === "codex";

  const optionSource =
    status?.sessionModelOptions ?? discoveredOptions ?? null;

  useEffect(() => {
    if (!optionSource) return;
    const saved = useAcpSettingsStore.getState();
    const patch = mergeSessionModelDefaults(
      {
        modelId: saved.modelId ?? "",
        reasoningEffort: saved.reasoningEffort ?? "",
      },
      optionSource,
    );
    if (Object.keys(patch).length > 0) {
      patchSettings(patch);
    }
  }, [optionSource, patchSettings]);

  const modelOptions = useMemo(
    () => buildModelOptions(optionSource, modelId),
    [modelId, optionSource],
  );
  const reasoningOptions = useMemo(
    () => buildReasoningOptions(optionSource, reasoningEffort),
    [optionSource, reasoningEffort],
  );

  const refreshStatus = () => {
    void queryClient.invalidateQueries({ queryKey: ["acp-status"] });
  };

  const applyModelSelection = async (next: {
    modelId?: string;
    reasoningEffort?: string;
  }) => {
    patchSettings(next);
    if (!sessionConnected || busy) return;

    try {
      await acpSetSessionModel({
        modelId: next.modelId ?? modelId,
        reasoningEffort: next.reasoningEffort ?? reasoningEffort,
      });
      await queryClient.invalidateQueries({ queryKey: ["acp-status"] });
    } catch (error) {
      setDiscoverError(errorMessage(error));
    }
  };

  const discoverModels = async () => {
    setDiscoverBusy(true);
    setDiscoverError(null);
    try {
      const result = await discoverAcpModels(
        profilesHintFromStore(activeProfileId, profiles),
        activeProfileId,
      );
      if (result.connected) {
        setDiscoveredOptions(result.options);
        const patch = mergeSessionModelDefaults(
          { modelId, reasoningEffort },
          result.options,
        );
        if (Object.keys(patch).length > 0) {
          patchSettings(patch);
        }
      } else {
        setDiscoverError(result.message);
      }
    } catch (error) {
      setDiscoverError(errorMessage(error));
    } finally {
      setDiscoverBusy(false);
    }
  };

  const saveCustomProfile = () => {
    const command = customCommand.trim();
    if (!command) return;
    upsertProfile({
      id: "custom",
      name: "自定义 ACP",
      kind: "Custom",
      command,
      args: [],
      env: {},
    });
    setActiveProfileId("custom");
    refreshStatus();
  };

  const controlsDisabled = busy || discoverBusy;
  const hasModelOptions = modelOptions.length > 0;
  const hasReasoningOptions = reasoningOptions.length > 0;

  return (
    <ChatColumn className="shrink-0 border-t border-border py-2">
      <button
        type="button"
        className="flex w-full items-center justify-between text-[11px] text-muted-foreground hover:text-foreground"
        onClick={() => setOpen((value) => !value)}
      >
        <span>Agent 设置</span>
        <span>{open ? "收起 ▲" : "展开 ▼"}</span>
      </button>

      {open ? (
        <div className="mt-2 space-y-3">
          <Field label="Agent">
            <select
              className={selectClassName}
              value={activeProfileId}
              disabled={controlsDisabled}
              onChange={(e) => {
                setActiveProfileId(e.target.value);
                setDiscoveredOptions(null);
                refreshStatus();
              }}
            >
              {profileList.map((profile) => (
                <option key={profile.id} value={profile.id}>
                  {profileLabel(profile)}
                </option>
              ))}
            </select>
          </Field>

          {activeProfileId === "custom" ? (
            <div className="flex gap-2">
              <input
                className="h-8 min-w-0 flex-1 rounded-md border border-border bg-background px-2 text-xs"
                placeholder="自定义 Agent 命令"
                value={customCommand}
                onChange={(e) => setCustomCommand(e.target.value)}
              />
              <Button
                size="sm"
                variant="outline"
                disabled={!customCommand.trim() || controlsDisabled}
                onClick={saveCustomProfile}
              >
                保存
              </Button>
            </div>
          ) : null}

          <Field label="模型">
            {hasModelOptions ? (
              <select
                className={selectClassName}
                value={modelId}
                disabled={controlsDisabled}
                onChange={(e) => {
                  void applyModelSelection({ modelId: e.target.value });
                }}
              >
                <option value="">Agent 默认</option>
                {modelOptions.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.name}
                  </option>
                ))}
              </select>
            ) : (
              <input
                className={selectClassName}
                value={modelId}
                disabled={controlsDisabled}
                placeholder={
                  sessionConnected
                    ? "当前 Agent 未提供模型列表，可手动输入模型 ID"
                    : "连接 Agent 后可选择，或先检测可用模型"
                }
                onChange={(e) => patchSettings({ modelId: e.target.value })}
                onBlur={() => {
                  if (modelId.trim()) {
                    void applyModelSelection({ modelId });
                  }
                }}
              />
            )}
          </Field>

          {hasReasoningOptions ? (
            <Field label="思考程度">
              <select
                className={selectClassName}
                value={reasoningEffort}
                disabled={controlsDisabled}
                onChange={(e) => {
                  void applyModelSelection({ reasoningEffort: e.target.value });
                }}
              >
                <option value="">Agent 默认</option>
                {reasoningOptions.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.name}
                  </option>
                ))}
              </select>
            </Field>
          ) : null}

          {!sessionConnected && status?.available ? (
            <Button
              size="sm"
              variant="outline"
              className="h-7 w-full text-[11px]"
              disabled={controlsDisabled}
              onClick={() => {
                void discoverModels();
              }}
            >
              {discoverBusy ? "正在检测可用模型…" : "检测可用模型"}
            </Button>
          ) : null}

          <Field label="模式">
            <select
              className={selectClassName}
              value={agentMode}
              disabled={controlsDisabled}
              onChange={(e) => patchSettings({ agentMode: e.target.value })}
            >
              {AGENT_MODES.map((mode) => (
                <option key={mode.id} value={mode.id}>
                  {mode.label}
                </option>
              ))}
            </select>
          </Field>

          <Field label="权限">
            <select
              className={selectClassName}
              value={permissionMode}
              disabled={controlsDisabled}
              onChange={(e) =>
                patchSettings({
                  permissionMode: e.target.value as PermissionMode,
                })
              }
            >
              <option value="auto">自动批准（类似 Cursor Auto-run）</option>
              <option value="ask">每次询问</option>
            </select>
          </Field>

          <Field label="思考展示">
            <select
              className={selectClassName}
              value={thinkingLevel}
              disabled={controlsDisabled}
              onChange={(e) =>
                patchSettings({
                  thinkingLevel: e.target.value as ThinkingLevel,
                })
              }
            >
              <option value="hidden">隐藏</option>
              <option value="minimal">流式时显示，完成后收起</option>
              <option value="verbose">始终保留思考/工具轨迹</option>
            </select>
          </Field>

          {discoverError ? (
            <p className="text-[10px] leading-relaxed text-destructive">
              {discoverError}
            </p>
          ) : null}

          {status?.codexPath ? (
            <p className="text-[10px] leading-relaxed text-muted-foreground">
              本机 Codex：{status.codexPath}
            </p>
          ) : null}

          {status?.codexConfigFound === false && activeProfileId === "codex" ? (
            <p className="text-[10px] leading-relaxed text-muted-foreground">
              未检测到 %USERPROFILE%\.codex 配置；若已在终端登录 Codex，请重启 Lumina 后再试。
            </p>
          ) : null}

          {status?.hint ? (
            <p className="text-[11px] leading-relaxed text-muted-foreground">
              {status.hint}
            </p>
          ) : null}

          {showResponsesNote && status?.responsesOnlyNote ? (
            <p className="text-[10px] leading-relaxed text-muted-foreground">
              {status.responsesOnlyNote}
            </p>
          ) : null}
        </div>
      ) : null}
    </ChatColumn>
  );
}

function Field({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <label className="block space-y-1">
      <span className="text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
        {label}
      </span>
      {children}
    </label>
  );
}

function profileLabel(profile: AgentProfileStatus): string {
  const mark = profile.available ? "✓" : "×";
  return `${mark} ${profile.name}`;
}
