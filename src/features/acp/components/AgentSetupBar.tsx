import { useEffect, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";

import { Button } from "@/components/ui/button";
import { errorMessage } from "@/lib/format";

import { setActiveAcpProfile, upsertAcpProfile } from "../api";
import type { AcpStatus, AgentProfileStatus } from "../types";

type Props = {
  status: AcpStatus | undefined;
  loading?: boolean;
  busy?: boolean;
  onStatusError?: (message: string) => void;
};

/** Compact agent picker — full config stays here; chat is separate. */
export function AgentSetupBar({ status, loading, busy, onStatusError }: Props) {
  const queryClient = useQueryClient();
  const [open, setOpen] = useState(false);
  const [activeId, setActiveId] = useState("codex");
  const [customCommand, setCustomCommand] = useState("");

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
  const showResponsesNote =
    active?.kind === "Codex" || activeId === "codex";

  return (
    <div className="shrink-0 space-y-2 border-b border-border px-3 py-2">
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
          {open ? "收起" : "设置"}
        </button>
      </div>

      {open ? (
        <div className="space-y-2">
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
    </div>
  );
}

function profileLabel(p: AgentProfileStatus): string {
  const mark = p.available ? "✓" : "×";
  return `${mark} ${p.name}`;
}
