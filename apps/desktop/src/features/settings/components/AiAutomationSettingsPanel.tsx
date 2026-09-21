import { useQuery } from "@tanstack/react-query";

import { profilesHintFromStore } from "@lumina/chat-ui/defaultAgentProfiles";
import { useAcpProfilesStore } from "@lumina/chat-ui/acpProfilesStore";

import { getAcpStatus } from "@/features/acp/api";
import { AgentSettingsPanel } from "@/features/acp/components/AgentSettingsPanel";
import { profilesSignature } from "@lumina/chat-ui/profilesSignature";

export function AiAutomationSettingsPanel() {
  const activeProfileId = useAcpProfilesStore((state) => state.activeProfileId);
  const profiles = useAcpProfilesStore((state) => state.profiles);
  const statusQuery = useQuery({
    queryKey: ["acp-status", activeProfileId, profilesSignature(profiles)],
    queryFn: () =>
      getAcpStatus(profilesHintFromStore(activeProfileId, profiles)),
    staleTime: 15_000,
  });

  return (
    <section className="space-y-3 rounded-lg border border-border bg-card/40 p-4">
      <div>
        <h2 className="text-base font-medium text-foreground">AI 与自动化</h2>
        <p className="mt-1 text-xs text-muted-foreground">
          使用现有 Agent 配置控制 AI 对话、字幕任务和章节生成；这里不创建第二套 Agent 状态。
        </p>
      </div>
      <AgentSettingsPanel status={statusQuery.data} />
    </section>
  );
}
