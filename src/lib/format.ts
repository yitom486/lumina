/** Shared frontend helpers. */

export function formatTime(ms: number): string {
  if (!Number.isFinite(ms) || ms < 0) return "0:00";
  const totalSec = Math.floor(ms / 1000);
  const m = Math.floor(totalSec / 60);
  const s = totalSec % 60;
  return `${m}:${s.toString().padStart(2, "0")}`;
}

/** Stable code → Chinese fallback when Rust message is missing or still English. */
const ERROR_FALLBACK: Record<string, string> = {
  // Player
  InitializationError: "播放引擎初始化失败",
  LoadError: "无法打开该媒体文件",
  UnsupportedMedia: "不支持该媒体格式",
  NativeWindowError: "视频窗口异常",
  PlaybackError: "播放操作失败，请重试",
  InvalidState: "当前状态无法执行该操作",
  InternalError: "内部错误，请重试",
  // Media
  ProbeNotFound: "未找到 ffprobe",
  FileNotFound: "找不到文件",
  ProbeFailed: "媒体探测失败",
  InvalidMedia: "无效的媒体文件",
  // Subtitle
  ToolNotFound: "未找到 ffmpeg",
  ExtractFailed: "字幕抽取失败",
  ParseFailed: "字幕解析失败",
  UnsupportedSubtitle: "不支持该字幕格式",
  NoSubtitleTrack: "没有可用字幕轨",
  // Asr
  NotConfigured: "未配置可选组件（ASR/ACP）",
  Busy: "已有任务在运行",
  TranscribeFailed: "语音转写失败",
  Cancelled: "已取消",
  // Acp
  SpawnFailed: "无法启动 ACP 进程",
  ProtocolError: "ACP 协议错误",
  // Note
  IoError: "笔记读写失败",
  NotFound: "找不到该笔记",
  InvalidNote: "笔记无效",
};

function hasCjk(text: string): boolean {
  return /[\u4e00-\u9fff]/.test(text);
}

/** Prefer Rust Chinese `message`; fall back by `code` if still English. */
export function formatPlayerError(error: {
  code?: string;
  message?: string;
  details?: string;
} | null | undefined): string {
  if (!error) return "发生未知错误";
  const message = error.message?.trim() ?? "";
  if (message && hasCjk(message)) return message;
  const byCode = error.code ? ERROR_FALLBACK[error.code] : undefined;
  if (byCode) return byCode;
  if (message) return message;
  return "发生未知错误";
}

export function errorMessage(error: unknown): string {
  if (typeof error === "object" && error && "code" in error && "message" in error) {
    return formatPlayerError(
      error as { code?: string; message?: string; details?: string },
    );
  }
  if (typeof error === "object" && error && "message" in error) {
    const message = String((error as { message: string }).message);
    return message;
  }
  return String(error);
}
