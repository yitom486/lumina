/** Actionable hint for a failed chat turn — never exposes tool names or stderr. */

export function hintForAcpFailure(code: string | undefined): string {
  switch (code) {
    case "ProtocolError":
      return "Windows 上请确认 Codex 已登录（codex login），且 Bun 能拉取 ACP 适配器；然后点击「新建对话」重试。";
    case "SpawnFailed":
      return "无法启动 Agent，请展开下方「Agent 设置」检查启动命令。";
    case "NotConfigured":
      return "请在终端运行 codex login 完成登录，或检查 %USERPROFILE%\\.codex 中的 API 配置。";
    default:
      return "可点击「新建对话」后重新提问；若仍失败请重启 Lumina。";
  }
}
