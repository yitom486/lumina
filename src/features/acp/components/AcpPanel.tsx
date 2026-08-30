import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";

import { Button } from "@/components/ui/button";
import { errorMessage } from "@/lib/format";
import { cn } from "@/lib/utils";

import {
  acpCancel,
  acpPrompt,
  getAcpStatus,
  setActiveAcpProfile,
  upsertAcpProfile,
} from "../api";
import type { AcpEvent, AgentProfileStatus } from "../types";

export function AcpPanel() {
  const queryClient = useQueryClient();
  const statusQuery = useQuery({
    queryKey: ["acp-status"],
    queryFn: getAcpStatus,
    staleTime: 15_000,
  });

  const [prompt, setPrompt] = useState("");
  const [log, setLog] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [customCommand, setCustomCommand] = useState("");
  const [activeId, setActiveId] = useState("codex");

  useEffect(() => {
    if (statusQuery.data) {
      setActiveId(statusQuery.data.activeProfileId);
      const custom = statusQuery.data.profiles.find((p) => p.id === "custom");
      if (custom) setCustomCommand(custom.command);
    }
  }, [statusQuery.data]);

  const push = (line: string) => setLog((prev) => [...prev, line]);

  const switchMutation = useMutation({
    mutationFn: setActiveAcpProfile,
    onSuccess: (status) => {
      queryClient.setQueryData(["acp-status"], status);
      setActiveId(status.activeProfileId);
    },
    onError: (error) => push(errorMessage(error)),
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
    onSuccess: (status) => {
      queryClient.setQueryData(["acp-status"], status);
      setActiveId(status.activeProfileId);
    },
    onError: (error) => push(errorMessage(error)),
  });

  const runMutation = useMutation({
    mutationFn: async (text: string) => {
      setBusy(true);
      setLog([]);
      return acpPrompt(
        text,
        (event: AcpEvent) => {
          switch (event.type) {
            case "started":
              push("会话已开始");
              break;
            case "progress":
              push(event.message);
              break;
            case "agentMessage":
              push(event.text);
              break;
            case "finished":
              push(`完成：${event.text}`);
              break;
            case "failed":
              push(`失败：${event.message}`);
              break;
          }
        },
        { profileId: activeId },
      );
    },
    onSettled: () => setBusy(false),
    onError: (error) => push(errorMessage(error)),
  });

  const status = statusQuery.data;
  const available = status?.available ?? false;
  const profiles = status?.profiles ?? [];
  const activeProfile = profiles.find((p) => p.id === activeId);
  const showResponsesNote =
    activeProfile?.kind === "Codex" || activeId === "codex";

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-2 p-3">
      <p className="text-xs text-muted-foreground">
        {statusQuery.isLoading
          ? "正在检测 ACP Agent…"
          : (status?.message ?? "无法获取 ACP 状态")}
      </p>

      <label className="text-[11px] text-muted-foreground">Agent</label>
      <select
        className="h-8 rounded-md border border-border bg-background px-2 text-xs"
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
            placeholder="自定义 ACP 可执行文件路径或命令名"
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
        <p className="text-[11px] leading-relaxed text-muted-foreground">{status.hint}</p>
      ) : null}

      {showResponsesNote && status?.responsesOnlyNote ? (
        <p className="rounded-md border border-border/60 bg-muted/20 px-2 py-1.5 text-[10px] leading-relaxed text-muted-foreground">
          {status.responsesOnlyNote}
        </p>
      ) : null}

      <div className="flex flex-wrap gap-2 text-[10px] text-muted-foreground">
        <span>适配器：{status?.adapterFound ? "已找到" : "未找到"}</span>
        <span>Codex：{status?.codexFound ? "已找到" : "未找到"}</span>
      </div>

      {status?.cliPath ? (
        <p className="truncate text-[10px] text-muted-foreground" title={status.cliPath}>
          {status.cliPath}
        </p>
      ) : null}

      <textarea
        className={cn(
          "min-h-[72px] w-full resize-none rounded-md border border-border bg-background px-2 py-1.5 text-sm",
          "outline-none focus-visible:ring-1 focus-visible:ring-ring",
        )}
        placeholder="向当前 ACP Agent 提问（可选功能）…"
        value={prompt}
        disabled={!available || busy}
        onChange={(e) => setPrompt(e.target.value)}
      />

      <div className="flex gap-2">
        <Button
          size="sm"
          disabled={!available || busy || !prompt.trim()}
          onClick={() => runMutation.mutate(prompt.trim())}
        >
          {busy ? "进行中…" : "发送"}
        </Button>
        <Button
          size="sm"
          variant="outline"
          disabled={!busy}
          onClick={() => {
            void acpCancel();
          }}
        >
          取消
        </Button>
      </div>

      <div className="min-h-0 flex-1 overflow-auto rounded-md border border-border bg-muted/30 p-2 text-xs whitespace-pre-wrap">
        {log.length === 0 ? (
          <span className="text-muted-foreground">尚无会话输出</span>
        ) : (
          log.map((line, i) => (
            <div key={`${i}-${line.slice(0, 12)}`} className="mb-1">
              {line}
            </div>
          ))
        )}
      </div>
    </div>
  );
}

function profileLabel(p: AgentProfileStatus): string {
  const mark = p.available ? "✓" : "×";
  return `${mark} ${p.name}`;
}
