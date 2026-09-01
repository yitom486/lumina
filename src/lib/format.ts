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
  ProbeNotFound: "媒体分析组件未就绪",
  FileNotFound: "找不到媒体文件或无法访问",
  ProbeFailed: "无法读取该视频的媒体信息",
  InvalidMedia: "该文件无法作为媒体使用",
  // Subtitle / Asr share ExtractFailed code — Rust message is authoritative
  ToolNotFound: "字幕工具未就绪",
  ExtractFailed: "内容提取失败",
  ParseFailed: "无法解析字幕",
  UnsupportedSubtitle: "不支持该字幕格式",
  NoSubtitleTrack: "该媒体没有可用字幕轨",
  // Asr
  NotConfigured: "未配置可选组件（ASR/ACP）",
  Busy: "已有任务在运行",
  TranscribeFailed: "语音转写失败",
  Cancelled: "已取消",
  // Acp
  SpawnFailed: "无法启动 AI Agent",
  ProtocolError: "与 Agent 通信失败",
  // Media library
  InvalidDirectory: "媒体目录不存在或无法访问",
  InvalidInput: "输入内容无效，请检查后重试",
  GroupNotFound: "找不到待处理的媒体分组",
  PrivacyConsentRequired: "请先确认允许发送文件名用于智能匹配",
  CredentialAccessFailed: "无法访问系统安全凭据，请检查系统账户后重试",
  ResolverNotConfigured: "未配置媒体智能匹配服务",
  AgentResolverFailed: "媒体匹配 Agent 不可用，请检查 Agent 配置",
  RemoteRequestFailed: "媒体信息查询失败，请稍后重试",
  InvalidResolverResponse: "智能匹配结果无效，请改用手动标题",
  ScanFailed: "媒体目录扫描失败，请重试",
  StorageFailed: "媒体索引保存失败，请重试",
  NotRunning: "媒体目录守护服务未启动",
  // Note
  IoError: "笔记读写失败",
  NotFound: "找不到该笔记",
  InvalidNote: "笔记无效",
};

function hasCjk(text: string): boolean {
  return /[\u4e00-\u9fff]/.test(text);
}

/** Prefer Rust business `message`; fall back by `code`. Never surface `details`. */
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
  if (!message) return "发生未知错误";
  return "操作失败，请重试";
}

/** User-visible error text only — ignores technical `details`. */
export function errorMessage(error: unknown): string {
  if (typeof error === "object" && error && "code" in error && "message" in error) {
    return formatPlayerError(
      error as { code?: string; message?: string; details?: string },
    );
  }
  if (typeof error === "object" && error && "message" in error) {
    return formatPlayerError({
      message: String((error as { message: string }).message),
    });
  }
  return "操作失败，请重试";
}
