import { useEffect, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";

import { Button } from "@/components/ui/button";

import { useAcpProfilesStore } from "../acpProfilesStore";
import { useAcpSettingsStore } from "../acpSettingsStore";
import type {
  AcpStatus,
  AgentProfileStatus,
  PermissionMode,
  ThinkingLevel,
} from "../types";
import { ChatColumn } from "./ChatShell";

type Props = {
  status: AcpStatus | undefined;
  busy?: boolean;
};

const AGENT_MODES = [
  { id: "default", label: "默认" },
  { id: "plan", label: "计划优先" },
  { id: "fast", label: "快速" },
] as const;

/** Agent profile + client prefs — collapsed at bottom of chat column. */
export function AgentSettingsPanel({ status, busy }: Props) {
  const queryClient = useQueryClient();
  const [open, setOpen] = useState(false);
  const [customCommand, setCustomCommand] = useState("");

  const activeProfileId = useAcpProfilesStore((s) => s.activeProfileId);
  const setActiveProfileId = useAcpProfilesStore((s) => s.setActiveProfileId);
  const upsertProfile = useAcpProfilesStore((s) => s.upsertProfile);
  const permissionMode = useAcpSettingsStore((s) => s.permissionMode);
  const thinkingLevel = useAcpSettingsStore((s) => s.thinkingLevel);
  const agentMode = useAcpSettingsStore((s) => s.agentMode);
  const patchSettings = useAcpSettingsStore((s) => s.patchSettings);

  const customProfileCommand = status?.profiles.find(
    (profile) => profile.id === "custom",
  )?.command;

  useEffect(() => {
    if (customProfileCommand !== undefined) {
      setCustomCommand(customProfileCommand);
    }
  }, [customProfileCommand]);

  const profiles = status?.profiles ?? [];
  const active = profiles.find((profile) => profile.id === activeProfileId);
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
              className="h-8 w-full rounded-md border border-border bg-background px-2 text-xs"
              value={activeProfileId}
              disabled={busy}
              onChange={(e) => {
                setActiveProfileId(e.target.value);
                refreshStatus();
              }}
            >
              {profiles.map((profile) => (
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
                disabled={!customCommand.trim() || busy}
                onClick={saveCustomProfile}
              >
                保存
              </Button>
            </div>
          ) : null}

          <Field label="模式">
            <select
              className="h-8 w-full rounded-md border border-border bg-background px-2 text-xs"
              value={agentMode}
              disabled={busy}
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
              className="h-8 w-full rounded-md border border-border bg-background px-2 text-xs"
              value={permissionMode}
              disabled={busy}
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
              className="h-8 w-full rounded-md border border-border bg-background px-2 text-xs"
              value={thinkingLevel}
              disabled={busy}
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
