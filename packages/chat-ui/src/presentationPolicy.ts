import type { ChatActivity } from "./types";

/** Public chat presentation differs by build, never by individual component. */
export type ChatPresentationMode = "development" | "release";

/**
 * A closed, safe projection of an ACP activity for the WebView.
 *
 * Raw ACP titles and details may contain parameters, file paths, JSON-RPC
 * payloads, or implementation errors. They intentionally do not cross this
 * boundary. Development mode keeps an allowlisted technical tool identifier
 * for diagnosis; release mode receives only a business-facing state.
 */
export function presentChatActivities(
  activities: readonly ChatActivity[],
  mode: ChatPresentationMode,
): ChatActivity[] {
  return activities.map((activity) => {
    if (activity.kind === "tool") {
      const identifier = safeToolIdentifier(activity);
      return {
        id: activity.id,
        kind: "tool",
        ...(activity.toolCallId ? { toolCallId: activity.toolCallId } : {}),
        title:
          mode === "development"
            ? identifier
              ? `工具：${identifier}`
              : "工具调用"
            : releaseToolLabel(identifier, activity.status),
        status: safeToolStatus(activity.status),
        text: mode === "development" ? debugToolSummary(activity.status) : undefined,
      };
    }

    if (activity.kind === "plan") {
      return {
        id: activity.id,
        kind: "plan",
        title: "处理计划",
        text:
          mode === "development"
            ? "正在规划处理步骤"
            : "正在分析该段内容",
      };
    }

    return {
      id: activity.id,
      kind: "thought",
      title: "思路整理",
      text:
        mode === "development"
          ? "正在整理回答思路"
          : "正在分析该段内容",
    };
  });
}

function safeToolIdentifier(activity: ChatActivity): string | null {
  for (const candidate of [activity.title, activity.toolCallId]) {
    const value = candidate?.trim() ?? "";
    if (/^(?:lumina|mcp|acp)_[a-z0-9_]{1,96}$/i.test(value)) {
      return value;
    }
  }
  return null;
}

function releaseToolLabel(identifier: string | null, status?: string): string {
  const completed = status === "completed" || status === "success";
  const prefix = completed ? "已" : "正在";
  const value = identifier?.toLowerCase() ?? "";
  if (value.includes("transcript") || value.includes("subtitle")) {
    return `${prefix}读取该段台词`;
  }
  if (value.includes("frame") || value.includes("capture")) {
    return `${prefix}分析画面`;
  }
  if (value.includes("playback") || value.includes("position")) {
    return `${prefix}获取播放位置`;
  }
  if (value.includes("library") || value.includes("episode")) {
    return `${prefix}读取剧集信息`;
  }
  if (value.includes("annotation") || value.includes("note")) {
    return `${prefix}整理批注`;
  }
  return `${prefix}分析该段内容`;
}

function safeToolStatus(status?: string): string | undefined {
  switch (status) {
    case "pending":
    case "running":
    case "in_progress":
    case "completed":
    case "success":
    case "failed":
    case "error":
    case "cancelled":
      return status;
    default:
      return undefined;
  }
}

function debugToolSummary(status?: string): string | undefined {
  switch (status) {
    case "pending":
      return "等待执行";
    case "running":
    case "in_progress":
      return "正在执行";
    case "completed":
    case "success":
      return "执行完成";
    case "failed":
    case "error":
      return "执行未完成";
    case "cancelled":
      return "已取消";
    default:
      return undefined;
  }
}
