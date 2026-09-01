import { describe, expect, it } from "vitest";
import { errorMessage, formatPlayerError, formatTime } from "./format";

describe("formatTime", () => {
  it("formats mm:ss", () => {
    expect(formatTime(0)).toBe("0:00");
    expect(formatTime(65_000)).toBe("1:05");
    expect(formatTime(-1)).toBe("0:00");
  });
});

describe("formatPlayerError", () => {
  it("prefers Chinese message from Rust", () => {
    expect(
      formatPlayerError({
        code: "LoadError",
        message: "无法打开该媒体文件",
        details: "raw",
      }),
    ).toBe("无法打开该媒体文件");
  });

  it("falls back by code when message is English", () => {
    expect(
      formatPlayerError({
        code: "ProbeFailed",
        message: "ffprobe exited with error",
      }),
    ).toBe("无法读取该视频的媒体信息");
  });

  it("handles null / empty", () => {
    expect(formatPlayerError(null)).toBe("发生未知错误");
    expect(formatPlayerError({})).toBe("发生未知错误");
  });

  it("covers player, asr, acp and note codes", () => {
    expect(formatPlayerError({ code: "NotConfigured" })).toBe(
      "未配置可选组件（ASR/ACP）",
    );
    expect(formatPlayerError({ code: "InvalidState" })).toBe(
      "当前状态无法执行该操作",
    );
    expect(formatPlayerError({ code: "SpawnFailed" })).toBe("无法启动 AI Agent");
    expect(formatPlayerError({ code: "ProtocolError" })).toBe("与 Agent 通信失败");
    expect(formatPlayerError({ code: "ToolNotFound" })).toBe("字幕工具未就绪");
  });

  it("never surfaces details even when present", () => {
    expect(
      formatPlayerError({
        code: "LoadError",
        message: "无法打开该媒体文件",
        details: "ffprobe json: boom",
      }),
    ).toBe("无法打开该媒体文件");
    expect(
      errorMessage({
        code: "ProbeFailed",
        message: "无法读取该视频的媒体信息",
        details: "serde error",
      }),
    ).toBe("无法读取该视频的媒体信息");
  });

  it("does not surface unstructured transport errors", () => {
    expect(errorMessage("invalid args `config`: missing field `profile_id`")).toBe(
      "操作失败，请重试",
    );
    expect(errorMessage({ message: "serde error: invalid type" })).toBe(
      "操作失败，请重试",
    );
  });
});

describe("errorMessage", () => {
  it("unwraps structured errors", () => {
    expect(errorMessage({ code: "Busy", message: "已有语音转写任务在运行" })).toBe(
      "已有语音转写任务在运行",
    );
  });
});
