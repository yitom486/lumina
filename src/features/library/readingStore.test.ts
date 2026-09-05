import { beforeEach, describe, expect, it } from "vitest";

import type { EpisodeFile } from "./types";
import {
  episodeReadingStatus,
  findContinueTarget,
  findNextEpisode,
  useReadingStore,
} from "./readingStore";

const EPISODES: EpisodeFile[] = [
  { season: 1, episode: 1, title: "开篇", path: "C:\\s\\e01.mkv" },
  { season: 1, episode: 2, title: "发展", path: "C:\\s\\e02.mkv" },
  { season: 1, episode: 3, title: "结局", path: "C:\\s\\e03.mkv" },
];

beforeEach(() => {
  useReadingStore.setState({ doneSet: {} });
});

describe("episodeReadingStatus", () => {
  it("manual completion wins over progress", () => {
    expect(
      episodeReadingStatus("C:\\s\\e01.mkv", true, {
        "C:\\s\\e01.mkv": 1,
      }),
    ).toBe("done");
  });

  it("progress implies reading; neither implies not-started", () => {
    expect(episodeReadingStatus("C:\\s\\e01.mkv", true, {})).toBe("reading");
    expect(episodeReadingStatus("C:\\s\\e01.mkv", false, {})).toBe(
      "not-started",
    );
  });
});

describe("findContinueTarget", () => {
  it("picks the first not-done episode", () => {
    const done = { "C:\\s\\e01.mkv": 1 };
    expect(
      findContinueTarget(EPISODES, (path) => path in done)?.path,
    ).toBe("C:\\s\\e02.mkv");
  });

  it("returns null when everything is done", () => {
    const done = {
      "C:\\s\\e01.mkv": 1,
      "C:\\s\\e02.mkv": 1,
      "C:\\s\\e03.mkv": 1,
    };
    expect(findContinueTarget(EPISODES, (path) => path in done)).toBeNull();
  });
});

describe("findNextEpisode", () => {
  it("returns the episode after the current file", () => {
    expect(findNextEpisode(EPISODES, "C:\\s\\e01.mkv")?.path).toBe(
      "C:\\s\\e02.mkv",
    );
    expect(findNextEpisode(EPISODES, "C:\\s\\e03.mkv")).toBeNull();
  });

  it("falls back to the first episode when current is unknown", () => {
    expect(findNextEpisode(EPISODES, null)?.path).toBe("C:\\s\\e01.mkv");
    expect(findNextEpisode(EPISODES, "C:\\x\\unknown.mkv")?.path).toBe(
      "C:\\s\\e01.mkv",
    );
    expect(findNextEpisode([], "C:\\s\\e01.mkv")).toBeNull();
  });
});

describe("useReadingStore", () => {
  it("marks done and unmarks without touching other paths", () => {
    const store = useReadingStore.getState();
    store.markDone("C:\\s\\e01.mkv");
    expect("C:\\s\\e01.mkv" in useReadingStore.getState().doneSet).toBe(true);
    store.markDone("C:\\s\\e02.mkv");
    store.unmarkDone("C:\\s\\e01.mkv");
    const doneSet = useReadingStore.getState().doneSet;
    expect("C:\\s\\e01.mkv" in doneSet).toBe(false);
    expect("C:\\s\\e02.mkv" in doneSet).toBe(true);
  });
});
