import { useEffect, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";

import { Button } from "@/components/ui/button";
import { errorMessage } from "@/lib/format";

import { useAcpSettingsStore } from "../acpSettingsStore";
import { useAcpSessionStore } from "../acpSessionStore";
import { setActiveAcpProfile, upsertAcpProfile } from "../api";
import type {
  AcpStatus,
  AgentProfileStatus,
  PermissionMode,
  ThinkingLevel,
} from "../types";
import { ChatColumn } from "./ChatShell";

type Props = {
  status: AcpStatus | undefined;
  loading?: boolean;
  busy?: boolean;
  onStatusError?: (message: string) => void;
};

const AGENT_MODES = [
  { id: "default", label: "默认" },
  { id: "plan", label: "计划优先" },
  { id: "fast", label: "快速" },
] as const;

/** Agent profile + Cursor-like client settings. */
export function AgentSettingsPanel({
  status,
  loading,
  busy,
  onStatusError,
}: Props) {
  const queryClient = useQueryClient();
  const [open, setOpen] = useState(false);
  const [activeId, setActiveId] = useState("codex");
  const [customCommand, setCustomCommand] = useState("");

  const permissionMode = useAcpSettingsStore((s) => s.permissionMode);
  const thinkingLevel = useAcpSettingsStore((s) => s.thinkingLevel);
  const agentMode = useAcpSettingsStore((s) => s.agentMode);
  const patchSettings = useAcpSettingsStore((s) => s.patchSettings);
  const savedSession = useAcpSessionStore((s) => s.savedSession);

  useEffect(() => {
    if (!status) return;
    setActiveId(status.activeProfileId);
    const custom = status.profiles.find((p) => p.id === "custom");
    if (custom) setCustomCommand(custom.command);
  }, [status]);

  const switchMutation = useMutation({
    mutationFn: setActiveAcpProfile,
    onSuccess: (next) => {
      queryClient.setQueryData(["acp-status"], next);
      setActiveId(next.activeProfileId);
    },
    onError: (error) => onStatusError?.(errorMessage(error)),
  });

  const saveCustomMutation = useMutation({
    mutationFn: async (command: string) => {
      await upsertAcpProfile({
        id: "custom",
        name: "自定义 ACP",
        kind: "Custom",
        command: command.trim(),
        args: [],
        env: {},
      });
      return setActiveAcpProfile("custom");
    },
    onSuccess: (next) => {
      queryClient.setQueryData(["acp-status"], next);
      setActiveId(next.activeProfileId);
    },
    onError: (error) => onStatusError?.(errorMessage(error)),
  });

  const profiles = status?.profiles ?? [];
  const active = profiles.find((p) => p.id === activeId);
  const showResponsesNote = active?.kind === "Codex" || activeId === "codex";

  return (
    <ChatColumn className="shrink-0 border-b border-border py-2">
      <div className="flex items-center gap-2">
        <p className="min-w-0 flex-1 truncate text-xs text-muted-foreground">
          {loading
            ? "正在检测 Agent…"
            : (status?.message ?? "无法获取 Agent 状态")}
        </p>
        <button
          type="button"
          className="shrink-0 text-[11px] text-muted-foreground underline-offset-2 hover:underline"
          onClick={() => setOpen((v) => !v)}
        >
          {open ? "收起" : "智能体设置"}
        </button>
      </div>

      {savedSession ? (
        <p className="mt-1 truncate text-[10px] text-muted-foreground">
          已记住会话 ID，下次提问将尝试 resume
        </p>
      ) : null}

      {open ? (
        <div className="mt-2 space-y-3">
          <Field label="Agent">
            <select
              className="h-8 w-full rounded-md border border-border bg-background px-2 text-xs"
              value={activeId}
              disabled={busy || switchMutation.isPending}
              onChange={(e) => {
                const id = e.target.value;
                setActiveId(id);
                switchMutation.mutate(id);
              }}
            >
              {profiles.map((p) => (
                <option key={p.id} value={p.id}>
                  {profileLabel(p)}
                </option>
              ))}
            </select>
          </Field>

          {activeId === "custom" ? (
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
                disabled={!customCommand.trim() || saveCustomMutation.isPending}
                onClick={() => saveCustomMutation.mutate(customCommand)}
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
              {AGENT_MODES.map((m) => (
                <option key={m.id} value={m.id}>
                  {m.label}
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

function profileLabel(p: AgentProfileStatus): string {
  const mark = p.available ? "✓" : "×";
  return `${mark} ${p.name}`;
}
