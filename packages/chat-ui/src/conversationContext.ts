import type {
  AgentSessionInfo,
  ResumeOutcome,
} from "./types";

/**
 * 历史真相唯一来源：Agent 侧 `session/list`。
 *
 * 本地不再维护会话注册表/转录存档/标题账本，只保留本文件里的纯显示换算
 * （列表行、恢复提示语）与内存态缓存（调用方各自持有，不落盘）。
 */

/** Pure gate shared by the history list and the load path. */
export function canSwitchHistoryConversation(input: {
  busy: boolean;
  creatingSession: boolean;
}): boolean {
  return !input.busy && !input.creatingSession;
}

/**
 * 列表结果能支撑哪种断言。分两层，因为「没翻完」只影响反向判断：
 * 已经出现在返回里的会话是确实存在的正向证据，与翻页是否穷尽无关；
 * 而「没出现」只有在翻完之后才能解释成不存在。
 *
 * 注意：busy 不再参与——切换/回答期间 Query 缓存里的已校验行依然有效，
 * 藏起来只会让列表“消失”。忙时禁的是点选（调用方的 switchBlocked）与
 * 新拉取（hook 的 enabled 门），不是展示。
 */
export type AgentSessionListTrust = {
  /** 命中可断言存在，可据此展示列表 */
  canMatch: boolean;
  /** 未命中可断言不存在 */
  canAssertMissing: boolean;
};

export function agentSessionListTrust(input: {
  hasData: boolean;
  verified: boolean;
  truncated: boolean;
}): AgentSessionListTrust {
  const canMatch = input.hasData && input.verified;
  return { canMatch, canAssertMissing: canMatch && !input.truncated };
}

export type HistoryThreadRow = {
  sessionId: string;
  title: string;
  updatedAtMs: number;
  /**
   * 空占位候选：无原生标题 + 自家 kind 标记。Codex 建线程不起名，
   * 名来自首条消息；我们的失败回退会凭空建出从没发过言的线程，
   * 恰好就是这个形状。外部线程（无 kind）永不标——无法判断，不碰。
   */
  isEmptyCandidate: boolean;
};

/**
 * 原生标题里属于脚手架回声的前缀：prompt 触发头、resource_link 文件名、
 * 播放信息块。这些是发给模型看的，不是给人看的。
 */
const SCAFFOLD_TITLE_MARKERS = ["【工具优先】", "[@", "媒体："];

function isScaffoldTitle(title: string): boolean {
  const trimmed = title.trim();
  return SCAFFOLD_TITLE_MARKERS.some((marker) =>
    trimmed.startsWith(marker),
  );
}

/**
 * `[@name]` 文件回声里提炼媒体标签：去扩展名、点换空格、截断。
 * 同一目录下各集文件名不同，提炼后每行可区分（哪集一眼可见）；
 * 提炼失败才回退通用标签。纯显示换算，不编内容。
 */
function mediaLabelFromFileEcho(title: string): string | null {
  const trimmed = title.trim();
  if (!trimmed.startsWith("[@")) return null;
  const end = trimmed.indexOf("]");
  if (end <= 2) return null;
  const label = trimmed
    .slice(2, end)
    .replace(/\.[a-z0-9]{2,5}$/i, "")
    .replace(/[.。_]+/g, " ")
    .replace(/\s+/g, " ")
    .trim()
    .slice(0, 42)
    .trim();
  return label || null;
}

/**
 * 原生会话条目 → 历史列表行。标题优先级：
 * 本轮见过的用户首句 ＞ 像人话的原生标题 ＞ 按自家 prompt 结构提炼。
 *
 * 脚手架回声不直接展示短 id：`【工具优先】`是 Lumina prompt 头的独家
 * 指纹（外部客户端写不出，比 kind 元数据更硬——老线程根本没有 kind），
 * `[@文件名]`/`媒体：`是我们的内容块形状。文件名回声提炼成媒体标签
 * （各集可区分），触发头回声标出来源；点开一次后一律由真实首句覆盖。
 */
export function historyThreadRows(
  sessions: AgentSessionInfo[] | null | undefined,
  titleOverrides?: Record<string, string>,
): HistoryThreadRow[] {
  if (!Array.isArray(sessions)) return [];
  return sessions
    .filter(
      (session): session is AgentSessionInfo =>
        !!session &&
        typeof session.sessionId === "string" &&
        session.sessionId.trim() !== "",
    )
    .map((session) => {
      const parsed = session.updatedAt
        ? Date.parse(session.updatedAt)
        : Number.NaN;
      let title = titleOverrides?.[session.sessionId];
      const nativeTitle = session.title?.trim();
      if (!title) {
        if (nativeTitle && !isScaffoldTitle(nativeTitle)) {
          title = nativeTitle;
        } else if (nativeTitle) {
          title =
            mediaLabelFromFileEcho(nativeTitle) ?? "Lumina 对话";
        } else {
          // 真无标题才回退：新线程带 kind，老外部线程给短 id。
          // 脚手架回声走上面分支，永不掉到这里。
          title =
            session.kind === "chat"
              ? "Lumina 对话"
              : `对话 ${session.sessionId.slice(0, 8)}`;
        }
      }
      return {
        sessionId: session.sessionId,
        title,
        updatedAtMs: Number.isFinite(parsed) ? parsed : 0,
        // 见过正文（override）的永不标空——只认原生证据。
        isEmptyCandidate:
          !nativeTitle &&
          session.kind === "chat" &&
          !titleOverrides?.[session.sessionId],
      };
    })
    .sort((a, b) => b.updatedAtMs - a.updatedAtMs);
}

/**
 * 恢复 AI 记忆后给用户看的提示。
 *
 * 「被占用」和「已不存在」必须是两句话：占用方（通常是另一个正在运行的 AI
 * 客户端）放手后那条对话还能恢复，统一说成「已不存在」等于谎报数据丢失。
 *
 * `outcome` 缺省时退回旧的 id 比对信号，这样后端还没带上结果也不会静默。
 */
export function resumeOutcomeNotice(input: {
  outcome: ResumeOutcome | null | undefined;
  sessionMatchedRequest: boolean;
}): string {
  switch (input.outcome) {
    case "resumed":
      return "已恢复该对话的 AI 记忆";
    case "occupied":
      return "该对话的 AI 记忆正被其它程序使用，已作为新对话继续；关闭其它 AI 客户端后可重新恢复";
    case "unavailable":
      return "该对话的 AI 记忆已不存在，已作为新对话继续";
    default:
      return input.sessionMatchedRequest
        ? "已恢复该对话的 AI 记忆"
        : "该对话的 AI 记忆已不存在，已作为新对话继续";
  }
}
