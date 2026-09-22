import { describe, expect, it } from "vitest";

import {
  buildModelOptions,
  buildReasoningOptions,
  isModelSelectionEmpty,
  mergeModelDefaults,
  normalizeModelSelection,
  resolveModelSelection,
  selectedOptionLabel,
} from "./modelConfig";
import type { AcpSessionModelOptions } from "./types";

const options = (
  overrides: Partial<AcpSessionModelOptions> = {},
): AcpSessionModelOptions => ({
  models: [
    { value: "gpt-4o-mini", name: "Mini" },
    { value: "gpt-4o-luna", name: "Luna" },
  ],
  reasoningEfforts: [{ value: "medium", name: "Medium" }],
  currentModelId: "gpt-4o-mini",
  currentReasoningEffort: "minimal",
  ...overrides,
});

describe("normalizeModelSelection / resolveModelSelection", () => {
  it("trims and nulls blanks on the way in and out", () => {
    expect(
      normalizeModelSelection({ modelId: "  x  ", reasoningEffort: null }),
    ).toEqual({ modelId: "x", reasoningEffort: "" });
    expect(resolveModelSelection({ modelId: "  ", reasoningEffort: "" })).toEqual({
      modelId: null,
      reasoningEffort: null,
    });
    expect(
      resolveModelSelection({ modelId: "mini", reasoningEffort: " high " }),
    ).toEqual({ modelId: "mini", reasoningEffort: "high" });
  });

  it("detects empty selections", () => {
    expect(isModelSelectionEmpty({ modelId: "", reasoningEffort: "" })).toBe(true);
    expect(isModelSelectionEmpty({ modelId: "x", reasoningEffort: "" })).toBe(false);
  });
});

describe("buildModelOptions / buildReasoningOptions", () => {
  it("keeps the saved model visible when it is missing from the session list", () => {
    expect(buildModelOptions(options(), "custom-model").map((o) => o.value)).toEqual([
      "custom-model",
      "gpt-4o-mini",
      "gpt-4o-luna",
    ]);
    expect(buildModelOptions(options(), "gpt-4o-luna").map((o) => o.value)).toEqual([
      "gpt-4o-mini",
      "gpt-4o-luna",
    ]);
  });

  it("keeps saved values visible when disconnected", () => {
    expect(buildModelOptions(null, "gpt-4o-luna").map((o) => o.value)).toEqual([
      "gpt-4o-luna",
    ]);
    expect(buildReasoningOptions(undefined, "high").map((o) => o.value)).toEqual([
      "high",
    ]);
    expect(buildModelOptions(null, "  ")).toEqual([]);
  });
});

describe("mergeModelDefaults", () => {
  it("does not overwrite saved values", () => {
    expect(
      mergeModelDefaults(
        { modelId: "gpt-4o-luna", reasoningEffort: "medium" },
        options(),
      ),
    ).toEqual({});
  });

  it("fills defaults only when saved values are empty", () => {
    expect(
      mergeModelDefaults({ modelId: "", reasoningEffort: "" }, options()),
    ).toEqual({ modelId: "gpt-4o-mini", reasoningEffort: "minimal" });
  });

  it("falls back to the first model and skips missing reasoning defaults", () => {
    expect(
      mergeModelDefaults(
        { modelId: "", reasoningEffort: "" },
        options({ currentModelId: null, currentReasoningEffort: null }),
      ),
    ).toEqual({ modelId: "gpt-4o-mini" });
  });

  it("does nothing without session options", () => {
    expect(
      mergeModelDefaults({ modelId: "", reasoningEffort: "" }, null),
    ).toEqual({});
  });
});

describe("selectedOptionLabel", () => {
  it("prefers list names and falls back to the empty copy", () => {
    const list = buildModelOptions(options(), "");
    expect(selectedOptionLabel(list, "gpt-4o-mini", "Agent 默认")).toBe("Mini");
    expect(selectedOptionLabel(list, "typed-manually", "Agent 默认")).toBe(
      "typed-manually",
    );
    expect(selectedOptionLabel(list, "  ", "Agent 默认")).toBe("Agent 默认");
  });
});
