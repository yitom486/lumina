import { describe, expect, it } from "vitest";

import {
  buildModelOptions,
  buildReasoningOptions,
  classifyCursorDimensions,
  currentBooleanValue,
  currentSelectValue,
  isListedConfigValue,
  isModelSelectionEmpty,
  mergeModelDefaults,
  normalizeModelSelection,
  resolveModelSelection,
  selectedOptionLabel,
  withCurrentOption,
} from "./modelConfig";
import type { AcpSessionModelOptions, SessionConfigOption } from "./types";

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
  extraOptions: [],
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

function selectOption(
  partial: Partial<SessionConfigOption> & {
    id: string;
    values: string[];
    current?: string | null;
  },
): SessionConfigOption {
  const { values, current, ...rest } = partial;
  return {
    name: partial.id,
    description: null,
    category: null,
    ...rest,
    kind: {
      kind: "select",
      options: values.map((value) => ({
        value,
        name: value,
        description: null,
      })),
      current: current ?? null,
    },
  };
}

function booleanOption(
  partial: Partial<SessionConfigOption> & { id: string; current: boolean },
): SessionConfigOption {
  const { current, ...rest } = partial;
  return {
    name: partial.id,
    description: null,
    category: null,
    ...rest,
    kind: { kind: "boolean", current },
  };
}

describe("classifyCursorDimensions", () => {
  it("maps the five cursor dimensions and keeps display order", () => {
    const dims = classifyCursorDimensions([
      selectOption({
        id: "session-mode",
        category: "mode",
        values: ["agent", "plan"],
        current: "agent",
      }),
      selectOption({ id: "model", values: ["grok-4.7"], current: "grok-4.7" }),
      selectOption({
        id: "reasoning-effort",
        values: ["low", "high"],
        current: "high",
      }),
      selectOption({
        id: "context",
        category: "context",
        values: ["256k"],
      }),
      booleanOption({ id: "fast", current: true }),
    ]);
    expect(dims.mode?.id).toBe("session-mode");
    expect(dims.model?.id).toBe("model");
    expect(dims.effort?.id).toBe("reasoning-effort");
    expect(dims.context?.id).toBe("context");
    expect(dims.fastToggle?.id).toBe("fast");
    expect(dims.others).toEqual([]);
  });

  it("matches by id/name keywords and excludes collab modes", () => {
    const dims = classifyCursorDimensions([
      selectOption({ id: "mode-picker", values: ["a"] }),
      selectOption({ id: "model=grok-4.7", values: ["model=grok-4.7"] }),
      selectOption({ id: "thinking", values: ["x"] }),
      selectOption({ id: "ctx_window", values: ["y"] }),
    ]);
    expect(dims.mode?.id).toBe("mode-picker");
    expect(dims.model?.id).toBe("model=grok-4.7");
    expect(dims.effort?.id).toBe("thinking");
    expect(dims.context?.id).toBe("ctx_window");

    const collab = classifyCursorDimensions([
      selectOption({ id: "collab-mode", values: ["a"] }),
    ]);
    expect(collab.mode).toBeUndefined();
    expect(collab.others.map((option) => option.id)).toEqual(["collab-mode"]);
  });

  it("treats absent dimensions as absent, never invents options", () => {
    const dims = classifyCursorDimensions([
      selectOption({ id: "model", values: ["grok-4.7"] }),
    ]);
    expect(dims.model?.id).toBe("model");
    expect(dims.mode).toBeUndefined();
    expect(dims.effort).toBeUndefined();
    expect(dims.context).toBeUndefined();
    expect(dims.fastToggle).toBeUndefined();
    expect(classifyCursorDimensions(null).others).toEqual([]);
  });

  it("routes select-shaped fast into the secondary slot", () => {
    const dims = classifyCursorDimensions([
      selectOption({ id: "fast", values: ["true", "false"] }),
    ]);
    expect(dims.fastToggle).toBeUndefined();
    expect(dims.fastSelect?.id).toBe("fast");
  });
});

describe("isListedConfigValue", () => {
  it("only lets advertised values through", () => {
    const model = selectOption({ id: "model", values: ["grok-4.7"] });
    expect(isListedConfigValue(model, "grok-4.7")).toBe(true);
    expect(isListedConfigValue(model, "grok-4.7[fast=false]")).toBe(false);
    expect(isListedConfigValue(model, "")).toBe(false);
    const fast = booleanOption({ id: "fast", current: false });
    expect(isListedConfigValue(fast, "true")).toBe(true);
    expect(isListedConfigValue(fast, "yes")).toBe(false);
    expect(
      isListedConfigValue(
        {
          id: "mystery",
          name: "mystery",
          description: null,
          category: null,
          kind: { kind: "unsupported" },
        },
        "anything",
      ),
    ).toBe(false);
  });

  it("reads current values with safe fallbacks", () => {
    expect(
      currentSelectValue(
        selectOption({ id: "model", values: ["a"], current: "a" }),
      ),
    ).toBe("a");
    expect(currentSelectValue(selectOption({ id: "model", values: ["a"] }))).toBe(
      "",
    );
    expect(currentBooleanValue(booleanOption({ id: "fast", current: true }))).toBe(
      true,
    );
  });
});

describe("withCurrentOption", () => {
  it("swaps only the targeted dimension for optimistic display", () => {
    const before = options({
      extraOptions: [
        selectOption({ id: "model", values: ["a", "b"], current: "a" }),
        booleanOption({ id: "fast", current: true }),
      ],
    });
    const after = withCurrentOption(before, "model", "b");
    expect(after.extraOptions?.[0]?.kind).toMatchObject({ current: "b" });
    expect(after.extraOptions?.[1]?.kind).toMatchObject({ current: true });
    // 输入不动。
    expect(before.extraOptions?.[0]?.kind).toMatchObject({ current: "a" });

    const toggled = withCurrentOption(before, "fast", "false");
    expect(toggled.extraOptions?.[1]?.kind).toMatchObject({ current: false });
  });
});
