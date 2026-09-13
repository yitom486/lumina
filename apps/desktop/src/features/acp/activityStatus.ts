import type { ChatActivity } from "./types";

export function isToolRunning(status?: string): boolean {
  switch (status) {
    case "in_progress":
    case "running":
    case "pending":
      return true;
    default:
      return false;
  }
}

export function hasActiveToolActivity(activities: ChatActivity[]): boolean {
  return activities.some(
    (item) => item.kind === "tool" && isToolRunning(item.status),
  );
}

export function waitingLabel(activities: ChatActivity[]): string {
  if (hasActiveToolActivity(activities)) return "Agent 正在调用工具…";
  if (activities.some((item) => item.kind === "thought")) return "Agent 正在思考…";
  if (activities.some((item) => item.kind === "plan")) return "Agent 正在规划…";
  return "Agent 正在回复…";
}
