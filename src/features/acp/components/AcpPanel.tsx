import { useMutation, useQuery } from "@tanstack/react-query";
import { useState } from "react";

import { Button } from "@/components/ui/button";
import { errorMessage } from "@/lib/format";
import { cn } from "@/lib/utils";

import { acpCancel, acpPrompt, getAcpStatus } from "../api";
import type { AcpEvent } from "../types";

export function AcpPanel() {
  const statusQuery = useQuery({
    queryKey: ["acp-status"],
    queryFn: getAcpStatus,
    staleTime: 30_000,
  });

  const [prompt, setPrompt] = useState("");
  const [log, setLog] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);

  const push = (line: string) => setLog((prev) => [...prev, line]);

  const runMutation = useMutation({
    mutationFn: async (text: string) => {
      setBusy(true);
      setLog([]);
      return acpPrompt(text, (event: AcpEvent) => {
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
      });
    },
    onSettled: () => setBusy(false),
    onError: (error) => push(errorMessage(error)),
  });

  const available = statusQuery.data?.available ?? false;

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-2 p-3">
      <p className="text-xs text-muted-foreground">
        {statusQuery.isLoading
          ? "正在检测 ACP…"
          : (statusQuery.data?.message ?? "无法获取 ACP 状态")}
      </p>
      {statusQuery.data?.cliPath ? (
        <p className="truncate text-[10px] text-muted-foreground" title={statusQuery.data.cliPath}>
          {statusQuery.data.cliPath}
        </p>
      ) : null}

      <textarea
        className={cn(
          "min-h-[72px] w-full resize-none rounded-md border border-border bg-background px-2 py-1.5 text-sm",
          "outline-none focus-visible:ring-1 focus-visible:ring-ring",
        )}
        placeholder="向 Codex ACP 提问（可选功能）…"
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
