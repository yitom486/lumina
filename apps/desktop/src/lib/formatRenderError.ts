/** Map render-time failures to fixed business copy (no stack traces in UI). */

export type RenderErrorCopy = {
  title: string;
  message: string;
  hint: string;
};

export function formatRenderError(
  error: unknown,
  panelLabel = "面板",
): RenderErrorCopy {
  const raw =
    error instanceof Error ? error.message : error ? String(error) : "";

  if (raw.includes("Maximum update depth")) {
    return {
      title: `${panelLabel}暂时无法显示`,
      message: "界面状态发生冲突，对话面板无法完成加载。",
      hint: "请点击「重试」。若问题仍在，请完全退出并重新打开 Lumina。",
    };
  }

  if (
    raw.includes("reading 'map'") ||
    raw.includes("reading 'find'") ||
    raw.includes("reading 'filter'")
  ) {
    return {
      title: `${panelLabel}配置异常`,
      message: "本地 Agent 或对话设置不完整，无法渲染该面板。",
      hint: "请点击「重试」。若仍失败，请重启应用（将恢复默认 Agent 配置）。",
    };
  }

  if (raw.includes("is not a function")) {
    return {
      title: `${panelLabel}设置异常`,
      message: "智能体偏好未能正确加载。",
      hint: "请重启 Lumina 后再打开该标签页。",
    };
  }

  return {
    title: `${panelLabel}加载失败`,
    message: "该功能暂时不可用，其它区域仍可正常使用。",
    hint: "请点击「重试」，或先切换到其它标签页。",
  };
}
