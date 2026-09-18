import { useEffect, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";

import { Button } from "@lumina/ui/button";

import { useAcpProfilesStore } from "@lumina/chat-ui/acpProfilesStore";
import { clientSettingsFromStore, useAcpSettingsStore } from "@lumina/chat-ui/acpSettingsStore";
import type { AgentProfileStatus, ThinkingLevel } from "../types";
import { acpLoginAntigravity, acpSyncMcpCapabilities } from "../api";
import { useAgentModelControls } from "../useAgentModelControls";
import { ChatColumn } from "@lumina/chat-ui/components/ChatShell";

type Props = {
  status: import("../types").AcpStatus | undefined;
  busy?: boolean;
  sessionConnected?: boolean;
  sessionCwd?: string;
};

const AGENT_MODES = [
  { id: "default", label: "默认" },
  { id: "plan", label: "计划优先" },
  { id: "fast", label: "快速" },
] as const;

const selectClassName =
  "h-8 w-full rounded-md border border-border bg-background px-2 text-xs";

/** Advanced Agent prefs — model/permission live in ChatComposerBar. */
export function AgentSettingsPanel({
  status,
  busy,
  sessionConnected,
  sessionCwd,
}: Props) {
  const queryClient = useQueryClient();
  const [open, setOpen] = useState(false);
  const [customCommand, setCustomCommand] = useState("");
  const [proxyPort, setProxyPort] = useState("7897");
  const [loginLoading, setLoginLoading] = useState(false);
  const [loginError, setLoginError] = useState<string | null>(null);
  const [loginSuccess, setLoginSuccess] = useState<string | null>(null);

  const activeProfileId = useAcpProfilesStore((s) => s.activeProfileId);
  const setActiveProfileId = useAcpProfilesStore((s) => s.setActiveProfileId);
  const profiles = useAcpProfilesStore((s) => s.profiles);
  const upsertProfile = useAcpProfilesStore((s) => s.upsertProfile);
  const thinkingLevel = useAcpSettingsStore((s) => s.thinkingLevel);
  const agentMode = useAcpSettingsStore((s) => s.agentMode);
  const visionCapable = useAcpSettingsStore((s) => s.visionCapable);
  const patchSettings = useAcpSettingsStore((s) => s.patchSettings);

  const {
    controlsDisabled,
    controlError,
    discoverBusy,
    discoverModels,
    canDiscover,
  } = useAgentModelControls({
    status,
    busy,
    sessionConnected,
  });

  const customProfileCommand = status?.profiles.find(
    (profile) => profile.id === "custom",
  )?.command;

  const antigravityProfile = profiles.find((profile) => profile.id === "antigravity");

  useEffect(() => {
    if (customProfileCommand !== undefined) {
      setCustomCommand(customProfileCommand);
    }
  }, [customProfileCommand]);

  useEffect(() => {
    const savedPort = antigravityProfile?.env?.ACP_PROXY_PORT;
    if (savedPort) {
      setProxyPort(savedPort);
    }
  }, [antigravityProfile?.env?.ACP_PROXY_PORT]);

  const profileList = status?.profiles ?? [];
  const active = profileList.find((profile) => profile.id === activeProfileId);
  const showResponsesNote =
    active?.kind === "Codex" || activeProfileId === "codex";

  const refreshStatus = () => {
    void queryClient.invalidateQueries({ queryKey: ["acp-status"] });
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

  const handleAntigravityLogin = async () => {
    setLoginLoading(true);
    setLoginError(null);
    setLoginSuccess(null);
    try {
      const portNumber = Number.parseInt(proxyPort, 10);
      const validPort =
        Number.isInteger(portNumber) && portNumber > 0 && portNumber <= 65535
          ? portNumber
          : 7897;
      if (antigravityProfile) {
        upsertProfile({
          ...antigravityProfile,
          env: {
            ...antigravityProfile.env,
            ACP_PROXY_PORT: String(validPort),
          },
        });
      }
      const msg = await acpLoginAntigravity(validPort);
      setLoginSuccess(msg || "登录授权成功！");
      refreshStatus();
    } catch (err: unknown) {
      setLoginError(
        typeof err === "string"
          ? err
          : (err as { message?: string })?.message || "登录授权失败，请重试",
      );
    } finally {
      setLoginLoading(false);
    }
  };

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
                // Model ids are agent-specific (e.g. GPT-5.6 belongs to
                // Codex); never leak a saved selection into another agent.
                patchSettings({ modelId: "", reasoningEffort: "" });
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

          {activeProfileId === "antigravity" ? (
            <div className="space-y-2 rounded-md border border-border/60 bg-muted/20 p-2.5">
              <Field label="HTTP 代理端口（默认 7897）">
                <div className="flex gap-2">
                  <input
                    type="number"
                    className="h-8 min-w-0 flex-1 rounded-md border border-border bg-background px-2 text-xs"
                    placeholder="7897"
                    value={proxyPort}
                    onChange={(e) => {
                      const val = e.target.value;
                      setProxyPort(val);
                      if (antigravityProfile) {
                        upsertProfile({
                          ...antigravityProfile,
                          env: {
                            ...antigravityProfile.env,
                            ACP_PROXY_PORT: val.trim() || "7897",
                          },
                        });
                        refreshStatus();
                      }
                    }}
                  />
                </div>
                <p className="text-[10px] text-muted-foreground">
                  Google 授权及 Gemini 接口走此 HTTP/SOCKS5 代理端口访问（Clash/v2ray/Sing-box 等，常见端口为 7897 或 7890）。
                </p>
              </Field>

              <div className="space-y-1.5 pt-1">
                <div className="flex items-center justify-between">
                  <span className="text-[11px] font-medium text-foreground">Google 账号授权</span>
                  <span className="text-[10px]">
                    {status?.antigravityCredentialsFound ? (
                      <span className="font-medium text-emerald-500">✓ 已授权</span>
                    ) : (
                      <span className="font-medium text-amber-500">! 未登录</span>
                    )}
                  </span>
                </div>

                <Button
                  size="sm"
                  variant={status?.antigravityCredentialsFound ? "outline" : "default"}
                  className="h-8 w-full text-xs"
                  disabled={loginLoading || controlsDisabled}
                  onClick={() => {
                    void handleAntigravityLogin();
                  }}
                >
                  {loginLoading
                    ? "正在等待浏览器授权中..."
                    : status?.antigravityCredentialsFound
                      ? "重新授权 Google 账号"
                      : "登录 Google 账号"}
                </Button>

                {loginSuccess ? (
                  <p className="text-[10px] leading-relaxed text-emerald-500">
                    {loginSuccess}
                  </p>
                ) : null}

                {loginError ? (
                  <p className="text-[10px] leading-relaxed text-destructive">
                    {loginError}
                  </p>
                ) : null}
              </div>
            </div>
          ) : null}

          {canDiscover ? (
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

          <Field label="视图截图">
            <label className="flex cursor-pointer items-center gap-2 text-xs text-foreground">
              <input
                type="checkbox"
                className="size-3.5 rounded border border-border"
                checked={visionCapable}
                disabled={controlsDisabled}
                onChange={(e) => {
                  const nextVisionCapable = e.target.checked;
                  patchSettings({ visionCapable: nextVisionCapable });
                  if (sessionConnected) {
                    void acpSyncMcpCapabilities({
                      cwd: sessionCwd,
                      clientSettings: clientSettingsFromStore({
                        ...useAcpSettingsStore.getState(),
                        visionCapable: nextVisionCapable,
                      }),
                    });
                  }
                }}
              />
              启用画面截图工具（需视图模型）
            </label>
            <p className="text-[10px] leading-relaxed text-muted-foreground">
              以发送消息时的播放进度为锚点；默认单帧；前后秒数按约 1 帧/秒采样（单侧最多
              7 秒、共最多 15 帧）；640px JPEG；工具返回后本地即删。关闭后 MCP
              不再暴露截图工具；若 Agent 仍看不到变化，请点「新对话」。
            </p>
          </Field>

          {controlError ? (
            <p className="text-[10px] leading-relaxed text-destructive">
              {controlError}
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
