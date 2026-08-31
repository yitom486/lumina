import { describe, expect, it } from "vitest";

import { planResumeToast, seekForResume } from "./resumePlayback";

describe("planResumeToast", () => {
  it("shows toast only when playback landed near the saved spot", () => {
    expect(
      planResumeToast(579_000, 4_000_000, 579_500),
    ).toEqual({ kind: "toast", positionMs: 579_500 });

    expect(planResumeToast(579_000, 4_000_000, 0)).toEqual({ kind: "none" });
    expect(planResumeToast(579_000, 4_000_000, 12_000)).toEqual({
      kind: "none",
    });
  });

  it("skips toast when saved progress is not eligible", () => {
    expect(planResumeToast(2_000, 4_000_000, 2_000)).toEqual({ kind: "none" });
  });
});

describe("seekForResume", () => {
  it("retries until seek lands near the saved spot", async () => {
    let attempts = 0;
    const landed = await seekForResume(
      579_000,
      async () => {
        attempts += 1;
        if (attempts < 3) {
          return { currentTimeMs: 0 };
        }
        return { currentTimeMs: 579_200 };
      },
      [0, 0, 0],
    );

    expect(landed).toBe(579_200);
    expect(attempts).toBe(3);
  });

  it("returns null when every attempt fails", async () => {
    const landed = await seekForResume(
      579_000,
      async () => {
        throw new Error("demux not ready");
      },
      [0, 0],
    );

    expect(landed).toBeNull();
  });
});
