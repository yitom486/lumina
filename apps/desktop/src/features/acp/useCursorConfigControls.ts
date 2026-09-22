import { useMemo, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";

import { errorMessage } from "@/lib/format";

import { useAcpProfilesStore } from "@lumina/chat-ui/acpProfilesStore";
import {
  classifyCursorDimensions,
  isListedConfigValue,
  withCurrentOption,
  type CursorConfigDimensions,
} from "@lumina/chat-ui/modelConfig";
import type { AcpSessionModelOptions, AcpStatus } from "./types";
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
  // 下发中状态：UI 置灰防连点。State 管渲染，ref 管同 tick 双发的门
  // （两次调用发生在同一次渲染闭包里时 state 还没更新）。
  const [applying, setApplying] = useState(false);
  const applyingRef = useRef(false);
  // 待下发队列（按 configId 合并，只留最新意图）：cursor 单次切换固定
  // 3~6s，连点必须排队消化，不能静默吞掉——吞掉的点击在用户侧就是“坏了”。
  const queueRef = useRef<{ configId: string; value: string }[]>([]);

  const writeCachedOptions = (
    update: (options: AcpSessionModelOptions) => AcpSessionModelOptions,
  ) => {
    queryClient.setQueriesData<AcpStatus | undefined>(
      { queryKey: ["acp-status"] },
      (old) =>
        old?.sessionModelOptions ? { ...old, sessionModelOptions: update(old.sessionModelOptions) } : old,
    );
  };

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
  // cursor 切模型实测 4~8s、尾部更长：下发中整组置灰 + 单飞，
  // 不给第二次点击排队的机会（排队只会把等待翻倍）。
  const controlsDisabled = Boolean(busy || applying);

  const applyConfigOption = (configId: string, value: string) => {
    if (!configId.trim()) return;
    const option = (
      status?.sessionModelOptions?.extraOptions ?? []
    ).find((item) => item.id === configId);
    if (!option || !isListedConfigValue(option, value)) {
      setControlError("该选项值不在 Agent 下发列表中，未发送");
      return;
    }
    if (!sessionConnected || busy) return;
    // 先乐观落定（UI 零等待），再排队下发；失败靠失效重拉自愈。
    writeCachedOptions((options) => withCurrentOption(options, configId, value));
    queueRef.current = [
      ...queueRef.current.filter((item) => item.configId !== configId),
      { configId, value },
    ];
    void drainQueue();
  };

  const drainQueue = async () => {
    if (applyingRef.current) return;
    applyingRef.current = true;
    setApplying(true);
    try {
      setControlError(null);
      while (queueRef.current.length > 0) {
        const next = queueRef.current.shift();
        if (!next) break;
        const updated = await acpSetSessionConfig(next);
        writeCachedOptions(() => updated);
      }
      await queryClient.invalidateQueries({ queryKey: ["acp-status"] });
    } catch (error) {
      queueRef.current = [];
      setControlError(errorMessage(error));
      // 权威重拉：成功 applied 但回包丢失时以服务端为准，
      // 拒收时回到下发前的值。
      await queryClient.invalidateQueries({ queryKey: ["acp-status"] });
    } finally {
      applyingRef.current = false;
      setApplying(false);
    }
  };

  return {
    dims,
    hasCursorDims,
    controlsDisabled,
    controlError,
    applying,
    applyConfigOption,
  };
}
