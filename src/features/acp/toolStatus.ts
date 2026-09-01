export function isToolFailed(status?: string): boolean {
  switch (status) {
    case "failed":
    case "error":
    case "cancelled":
      return true;
    default:
      return false;
  }
}

export function isToolSucceeded(status?: string): boolean {
  switch (status) {
    case "completed":
    case "success":
      return true;
    default:
      return false;
  }
}

export function toolStatusLabel(status?: string): string {
  switch (status) {
    case "failed":
    case "error":
      return "失败";
    case "cancelled":
      return "已取消";
    case "completed":
    case "success":
      return "完成";
    case "in_progress":
    case "running":
      return "执行中";
    case "pending":
      return "等待中";
    default:
      return status?.trim() || "进行中";
  }
}

export function toolFailureHint(status?: string, detail?: string): string | null {
  if (!isToolFailed(status)) return null;
  const trimmed = detail?.trim();
  if (trimmed) return trimmed;
  return "工具执行未成功，Agent 将尝试其他方式继续";
}

export function mergeToolDetail(
  previous: string | undefined,
  next: string | undefined,
  append: boolean,
): string | undefined {
  const incoming = next?.trim();
  if (!incoming) return previous;
  if (!append || !previous?.trim()) return incoming;
  return `${previous.trim()}\n${incoming}`;
}
