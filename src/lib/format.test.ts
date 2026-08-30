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
    ).toBe("媒体探测失败");
  });

  it("handles null / empty", () => {
    expect(formatPlayerError(null)).toBe("发生未知错误");
    expect(formatPlayerError({})).toBe("发生未知错误");
  });

  it("covers player, asr, acp and note codes", () => {
    expect(formatPlayerError({ code: "NotConfigured" })).toBe("未配置可选组件（ASR/ACP）");
    expect(formatPlayerError({ code: "InvalidState" })).toBe(
      "当前状态无法执行该操作",
    );
    expect(formatPlayerError({ code: "SpawnFailed" })).toBe("无法启动 ACP 进程");
    expect(formatPlayerError({ code: "InvalidNote" })).toBe("笔记无效");
  });
});

describe("errorMessage", () => {
  it("unwraps structured errors", () => {
    expect(errorMessage({ code: "Busy", message: "已有 ASR 任务在运行" })).toBe(
      "已有 ASR 任务在运行",
    );
  });
});
