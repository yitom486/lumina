import { useMemo, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";

import { errorMessage } from "@/lib/format";

import { useAcpProfilesStore } from "@lumina/chat-ui/acpProfilesStore";
import {
  classifyCursorDimensions,
  isListedConfigValue,
  type CursorConfigDimensions,
} from "@lumina/chat-ui/modelConfig";
import type { AcpStatus } from "./types";
import { acpSetSessionConfig } from "./api";

type Options = {
  status: AcpStatus | undefined;
  busy?: boolean;
  sessionConnected?: boolean;
};

/**
 * 参数化 Agent（cursor）的输入栏控制：模式 / 模型 / 思考 / 上下文四个
 * 下拉 + Fast 开关。数据源是会话下发的 advertised 列表（缺席即不渲染），
 * 下发走 `session/set_config_option`，且只发 listed 原值——拼未 listed
 * 值会被拒收（`Invalid params`），门槛在发送前卡死。
 *
 * 会话级状态，不进 settings 持久化：agent 是自家默认值的唯一真相，
 * 下发成功后失效 `acp-status`，以服务端回读为准；失败只报一句，
 * 不做乐观 pending/回滚。
 */
export function useCursorConfigControls({
  status,
  busy,
  sessionConnected,
}: Options) {
  const queryClient = useQueryClient();
  const [controlError, setControlError] = useState<string | null>(null);

  const activeProfileId = useAcpProfilesStore((s) => s.activeProfileId);
  const isCursor = activeProfileId === "cursor";

  const dims: CursorConfigDimensions = useMemo(() => {
    if (!isCursor) return { others: [] };
    // 状态 scope 必须和当前画像一致：切画像瞬间旧 status 滞后一拍，
    // 拿错家的列表渲染下拉就是串台。
    const extra =
      status?.activeProfileId === activeProfileId
        ? (status?.sessionModelOptions?.extraOptions ?? [])
        : [];
    return classifyCursorDimensions(extra);
  }, [isCursor, status, activeProfileId]);

  // 有模型维度才算“参数化模式”：老 cursor（爆炸 variant 串）无维度，
  // 回落经典模型/思考下拉（下发什么展示什么）。
  const hasCursorDims = Boolean(dims.model);
  const controlsDisabled = Boolean(busy);

  const applyConfigOption = async (configId: string, value: string) => {
    if (!configId.trim()) return;
    const option = (
      status?.sessionModelOptions?.extraOptions ?? []
    ).find((item) => item.id === configId);
    if (!option || !isListedConfigValue(option, value)) {
      setControlError("该选项值不在 Agent 下发列表中，未发送");
      return;
    }
    if (!sessionConnected || busy) return;

    try {
      setControlError(null);
      await acpSetSessionConfig({ configId, value });
      await queryClient.invalidateQueries({ queryKey: ["acp-status"] });
    } catch (error) {
      setControlError(errorMessage(error));
    }
  };

  return {
    dims,
    hasCursorDims,
    controlsDisabled,
    controlError,
    applyConfigOption,
  };
}
