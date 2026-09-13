import { Button } from "@/components/ui/button";

import { respondAcpPermission } from "../api";
import type { PendingPermission } from "../types";
import { ChatColumn } from "./ChatShell";

type Props = {
  pending: PendingPermission;
  onDone: () => void;
};

export function PermissionPrompt({ pending, onDone }: Props) {
  const deny = pending.options.find(
    (o) => o.kind?.includes("reject") || o.kind?.includes("deny"),
  );
  const allow = pending.options.find((o) => o.kind?.startsWith("allow"));

  return (
    <ChatColumn className="rounded-md border border-amber-500/30 bg-amber-500/5 px-3 py-2">
      <p className="text-xs font-medium text-foreground">
        需要你的审批：{pending.title ?? pending.toolCallId ?? "工具调用"}
      </p>
      <div className="mt-2 flex flex-wrap gap-2">
        {allow ? (
          <Button
            size="sm"
            className="h-7 text-xs"
            onClick={() => {
              void respondAcpPermission(pending.requestId, allow.optionId).then(
                onDone,
              );
            }}
          >
            {allow.name || "允许"}
          </Button>
        ) : null}
        <Button
          size="sm"
          variant="outline"
          className="h-7 text-xs"
          onClick={() => {
            void respondAcpPermission(
              pending.requestId,
              deny?.optionId ?? null,
            ).then(onDone);
          }}
        >
          {deny?.name || "拒绝"}
        </Button>
      </div>
    </ChatColumn>
  );
}
