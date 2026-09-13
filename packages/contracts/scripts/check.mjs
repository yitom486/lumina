#!/usr/bin/env node
/**
 * Canonical contracts check (L0 §4.1).
 *
 * Validates that `packages/contracts/src/*.ts` mirrors the Rust serde shapes:
 * field names follow the container `rename_all` rule, TS `?` matches Rust
 * `Option<…>`, string-literal unions match enum variants, and the
 * adjacently-tagged `PlayerEvent` keeps its `type`/`payload` form.
 * Rust is the source of truth; TS is the mirror.
 *
 * Only Node builtins are used (`node:fs`, `node:path`, `node:url`), so the
 * script runs under both `bun` and `node` with no new dependencies.
 *
 * Ytdl / Library / full-Note DTOs are intentionally deferred (L0 §4.1
 * "稳定后再加入"): they are listed as warnings, never validated here.
 */

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = dirname(fileURLToPath(import.meta.url));
const ROOT = join(SCRIPT_DIR, "..", "..", "..");
const CONTRACTS = join(ROOT, "packages", "contracts", "src");

const issues = [];
const warnings = [];
let passed = 0;

function ok(message) {
  passed += 1;
  console.log(`  ✔ ${message}`);
}

function fail(message) {
  issues.push(message);
  console.log(`  ✘ ${message}`);
}

function warn(message) {
  warnings.push(message);
  console.log(`  ! ${message}`);
}

function read(rel) {
  return readFileSync(join(ROOT, rel), "utf8");
}

// --- case rules (mirror serde rename_all) ---

function capFirst(word) {
  return word.charAt(0).toUpperCase() + word.slice(1);
}

function toCamel(snake) {
  const parts = snake.split("_");
  return parts[0].charAt(0).toLowerCase() + parts[0].slice(1) + parts.slice(1).map(capFirst).join("");
}

function toPascal(snake) {
  return snake.split("_").map(capFirst).join("");
}

function applyRule(snake, rule) {
  if (rule === "camelCase") return toCamel(snake);
  if (rule === "PascalCase") return toPascal(snake);
  return snake;
}

// --- tiny scanners (quote-aware; no external parser) ---

function stripTsComments(src) {
  let out = "";
  let i = 0;
  let quote = null;
  while (i < src.length) {
    const c = src[i];
    if (quote) {
      out += c;
      if (c === "\\") {
        out += src[i + 1] ?? "";
        i += 2;
        continue;
      }
      if (c === quote) quote = null;
      i += 1;
      continue;
    }
    if (c === '"' || c === "'" || c === "`") {
      quote = c;
      out += c;
      i += 1;
      continue;
    }
    if (c === "/" && src[i + 1] === "*") {
      const end = src.indexOf("*/", i + 2);
      i = end === -1 ? src.length : end + 2;
      continue;
    }
    if (c === "/" && src[i + 1] === "/") {
      const end = src.indexOf("\n", i + 2);
      i = end === -1 ? src.length : end;
      continue;
    }
    out += c;
    i += 1;
  }
  return out;
}

/** Match the `{...}` block starting at `openIdx`; returns `{ body, end }`. */
function matchBraces(src, openIdx) {
  let depth = 0;
  let quote = null;
  for (let i = openIdx; i < src.length; i++) {
    const c = src[i];
    if (quote) {
      if (c === "\\") {
        i += 1;
        continue;
      }
      if (c === quote) quote = null;
      continue;
    }
    if (c === '"' || c === "'" || c === "`") {
      quote = c;
      continue;
    }
    if (c === "{") depth += 1;
    if (c === "}") {
      depth -= 1;
      if (depth === 0) return { body: src.slice(openIdx + 1, i), end: i };
    }
  }
  throw new Error("unbalanced braces");
}

/** Split on a separator char occurring at depth 0 outside strings. */
function splitTopLevel(body, sep) {
  const parts = [];
  let depth = 0;
  let quote = null;
  let current = "";
  for (let i = 0; i < body.length; i++) {
    const c = body[i];
    if (quote) {
      current += c;
      if (c === "\\") {
        current += body[i + 1] ?? "";
        i += 1;
        continue;
      }
      if (c === quote) quote = null;
      continue;
    }
    if (c === '"' || c === "'" || c === "`") {
      quote = c;
      current += c;
      continue;
    }
    if (c === "{" || c === "(" || c === "[") depth += 1;
    if (c === "}" || c === ")" || c === "]") depth -= 1;
    if (c === sep && depth === 0) {
      parts.push(current);
      current = "";
      continue;
    }
    current += c;
  }
  if (current.trim() !== "") parts.push(current);
  return parts;
}

// --- Rust parsing ---

function parseSerdeAttrs(attrText) {
  const out = {};
  const rule = attrText.match(/rename_all\s*=\s*"(\w+)"/);
  if (rule) out.renameAll = rule[1];
  const fieldsRule = attrText.match(/rename_all_fields\s*=\s*"(\w+)"/);
  if (fieldsRule) out.renameAllFields = fieldsRule[1];
  const rename = attrText.match(/rename\s*=\s*"([^"]+)"/);
  if (rename) out.rename = rename[1];
  out.hasDefault = /(?:^|,|\s)default(?:\s|,|=|$)/.test(attrText);
  const tag = attrText.match(/\btag\s*=\s*"([^"]+)"/);
  if (tag) out.tag = tag[1];
  const content = attrText.match(/\bcontent\s*=\s*"([^"]+)"/);
  if (content) out.content = content[1];
  return out;
}

function collectAttrLines(lines, idx) {
  // Collect the `#[serde(...)]` attribute above idx. A single attribute may
  // span multiple lines (tag/content/rename_all_fields on PlayerEvent), so we
  // walk upward from a closing `)]` tail while brackets stay unbalanced.
  const chunks = [];
  let depth = 0; // unbalanced `]` seen while walking upward
  for (let j = idx - 1; j >= 0; j--) {
    const t = lines[j].trim();
    if (t === "" || t.startsWith("///") || t.startsWith("//")) continue;
    if (t.startsWith("#[serde")) {
      chunks.unshift(t);
      depth += (t.match(/\]/g) ?? []).length - (t.match(/\[/g) ?? []).length;
      if (depth <= 0) break;
      continue;
    }
    if (t.startsWith("#[")) {
      // A different attribute (e.g. #[derive]) ends the serde zone.
      break;
    }
    if (depth > 0) {
      chunks.unshift(t);
      depth += (t.match(/\]/g) ?? []).length - (t.match(/\[/g) ?? []).length;
      continue;
    }
    if (t === ")]") {
      chunks.unshift(t);
      depth = 1;
      continue;
    }
    break;
  }
  const joined = chunks.join(" ");
  return joined.includes("#[serde") ? joined : "";
}

/** Parse `pub struct` / `pub enum` items: { kind, name, attrs, fields|variants }. */
function parseRustItems(src) {
  const lines = src.split("\n");
  const items = [];
  let i = 0;
  while (i < lines.length) {
    const m = lines[i].match(/^\s*pub\s+(struct|enum)\s+([A-Za-z0-9_]+)/);
    if (!m) {
      i += 1;
      continue;
    }
    const [, kind, name] = m;
    const attrs = parseSerdeAttrs(collectAttrLines(lines, i));
    // Find opening brace on this or following lines, then match it.
    const offset = lines.slice(0, i).join("\n").length + (i > 0 ? 1 : 0);
    const openIdx = src.indexOf("{", offset);
    const { body, end } = matchBraces(src, openIdx);
    if (kind === "struct") {
      items.push({ kind, name, attrs, fields: parseRustFields(body) });
    } else {
      items.push({ kind, name, attrs, variants: parseRustVariants(body) });
    }
    i = src.slice(0, end).split("\n").length;
  }
  return items;
}

function parseRustFields(body) {
  const fields = [];
  for (const part of splitTopLevel(body, ",")) {
    const lines = part.split("\n");
    let attrText = "";
    let decl = "";
    for (const line of lines) {
      const t = line.trim();
      if (t.startsWith("#[serde")) attrText += ` ${t}`;
      else if (t !== "" && !t.startsWith("///") && !t.startsWith("//")) decl += ` ${t}`;
    }
    const fm = decl.trim().match(/^(?:pub\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*:\s*(.+)$/);
    if (!fm) continue;
    fields.push({ name: fm[1], type: fm[2].trim(), attrs: parseSerdeAttrs(attrText) });
  }
  return fields;
}

function parseRustVariants(body) {
  const variants = [];
  for (const part of splitTopLevel(body, ",")) {
    const lines = part.split("\n").filter((l) => {
      const t = l.trim();
      return t !== "" && !t.startsWith("#[") && !t.startsWith("///") && !t.startsWith("//");
    });
    if (lines.length === 0) continue;
    const first = lines.join(" ").trim();
    const m = first.match(/^([A-Za-z0-9_]+)\s*(\{.*)?$/s);
    if (!m) continue;
    const inner = m[2];
    if (!inner) {
      variants.push({ name: m[1], fields: null });
      continue;
    }
    const openIdx = first.indexOf("{");
    const { body: innerBody } = matchBraces(first, openIdx);
    variants.push({ name: m[1], fields: parseRustFields(innerBody) });
  }
  return variants;
}

// --- TS parsing ---

function findTsType(src, name) {
  const clean = stripTsComments(src);
  const re = new RegExp(`export\\s+type\\s+${name}\\s*=`);
  const m = clean.match(re);
  if (!m) return null;
  let i = m.index + m[0].length;
  while (/\s/.test(clean[i])) i += 1;
  if (clean[i] === "{") {
    const { body, end } = matchBraces(clean, i);
    // Object type ends at the matching `}` of the *outer* level; ensure `;`.
    return { kind: "object", body, end };
  }
  // Alias: read to `;` at depth 0.
  let depth = 0;
  let quote = null;
  for (let j = i; j < clean.length; j++) {
    const c = clean[j];
    if (quote) {
      if (c === "\\") {
        j += 1;
        continue;
      }
      if (c === quote) quote = null;
      continue;
    }
    if (c === '"' || c === "'" || c === "`") {
      quote = c;
      continue;
    }
    if (c === "(" || c === "[") depth += 1;
    if (c === ")" || c === "]") depth -= 1;
    if (c === ";" && depth === 0) {
      return { kind: "alias", body: clean.slice(i, j).trim(), end: j };
    }
  }
  return null;
}

function parseTsFields(body) {
  const fields = [];
  for (const part of splitTopLevel(body, ";")) {
    for (const sub of splitTopLevel(part, ",")) {
      const t = sub.trim();
      if (t === "") continue;
      const m = t.match(/^([A-Za-z_][A-Za-z0-9_]*)\s*(\?)?:\s*([\s\S]+)$/);
      if (!m) continue;
      fields.push({ name: m[1], optional: m[2] === "?", type: m[3].trim() });
    }
  }
  return fields;
}

function parseStringLiterals(aliasBody) {
  const lits = [];
  const re = /"([^"]*)"/g;
  let m;
  while ((m = re.exec(aliasBody)) !== null) lits.push(m[1]);
  return lits;
}

// --- kind mapping (loose: names + nullability + casing, not full typing) ---

const PRIMITIVE = new Map([
  ["String", "string"],
  ["bool", "boolean"],
  ["u8", "number"],
  ["u16", "number"],
  ["u32", "number"],
  ["u64", "number"],
  ["u128", "number"],
  ["usize", "number"],
  ["i32", "number"],
  ["i64", "number"],
  ["f32", "number"],
  ["f64", "number"],
]);

const ENUM_AS_STRING = new Set([
  "PlayerErrorCode",
  "PlayerState",
  "SubtitleSource",
  "StreamKind",
  "MediaSourceKind",
]);

// Rust struct name -> TS type name when they intentionally differ.
const TYPE_ALIASES = new Map([["PlayerError", "PlayerErrorDto"]]);

function unwrapOption(rustType) {
  const m = rustType.trim().match(/^Option\s*<\s*([\s\S]+)\s*>$/);
  return m ? m[1].trim() : null;
}

function normalizeTsType(tsType) {
  return tsType
    .split("|")
    .map((p) => p.trim())
    .filter((p) => p !== "" && p !== "null" && p !== "undefined");
}

/** Check a Rust field type against a TS field type; returns a reason or null. */
function checkKind(rustType, tsType, fieldLabel) {
  const inner = unwrapOption(rustType) ?? rustType.trim();
  const alts = normalizeTsType(tsType);
  const vec = inner.match(/^Vec\s*<\s*(.+)\s*>$/);
  if (vec) {
    const elem = vec[1].trim();
    const elemTs = PRIMITIVE.get(elem) ?? elem;
    if (alts.some((a) => a === `${elemTs}[]`)) return null;
    return `${fieldLabel}: Rust Vec<${elem}> vs TS \`${tsType}\``;
  }
  if (PRIMITIVE.has(inner)) {
    if (alts.includes(PRIMITIVE.get(inner))) return null;
    return `${fieldLabel}: Rust ${inner} vs TS \`${tsType}\``;
  }
  if (ENUM_AS_STRING.has(inner)) {
    // Enums cross as `string`, a literal union, or a same-name TS reference.
    if (alts.includes("string") || alts.includes(inner) || alts.some((a) => a.startsWith('"'))) {
      return null;
    }
    return `${fieldLabel}: Rust enum ${inner} vs TS \`${tsType}\``;
  }
  const want = TYPE_ALIASES.get(inner) ?? inner;
  if (alts.includes(want)) return null;
  return `${fieldLabel}: Rust ${inner} vs TS \`${tsType}\``;
}

// --- coverage table: Rust item -> TS type (source of truth -> mirror) ---

const GROUPS = [
  {
    group: "errors",
    rustFile: "crates/lumina-player/src/error.rs",
    tsFile: "packages/contracts/src/errors.ts",
    pairs: [
      { rust: "PlayerError", ts: "AppError", kind: "struct" },
      { rust: "PlayerError", ts: "PlayerErrorDto", kind: "struct" },
    ],
  },
  {
    group: "player",
    rustFile: "crates/lumina-player/src/model.rs",
    tsFile: "packages/contracts/src/player.ts",
    pairs: [
      { rust: "PlayerSnapshot", ts: "PlayerSnapshot", kind: "struct" },
      { rust: "PlayerEvent", ts: "PlayerEvent", kind: "event" },
      { rust: "MediaSourceKind", ts: "MediaSourceKind", kind: "literals", rustFile: "crates/lumina-core/src/media_source.rs" },
    ],
  },
  {
    group: "subtitle",
    rustFile: "crates/lumina-subtitle/src/model.rs",
    tsFile: "packages/contracts/src/subtitle.ts",
    pairs: [
      { rust: "SubtitleSource", ts: "SubtitleSource", kind: "literals" },
      { rust: "SubtitleChoice", ts: "SubtitleChoice", kind: "struct" },
      { rust: "Cue", ts: "Cue", kind: "struct" },
      { rust: "Transcript", ts: "Transcript", kind: "struct" },
    ],
  },
  {
    group: "notes",
    rustFile: "crates/lumina-notes/src/model.rs",
    tsFile: "packages/contracts/src/notes.ts",
    pairs: [
      { rust: "NoteQuote", ts: "NoteQuote", kind: "struct" },
      { rust: "VideoAnnotationProposal", ts: "VideoAnnotationProposal", kind: "struct", rustFile: "crates/lumina-notes/src/proposal.rs" },
    ],
  },
  {
    group: "media",
    rustFile: "crates/lumina-media/src/model.rs",
    tsFile: "packages/contracts/src/media.ts",
    pairs: [{ rust: "MediaChapter", ts: "MediaChapter", kind: "struct" }],
  },
];

const DEFERRED_SCAN = [
  "apps/desktop/src/features/ytdl/api.ts",
  "apps/desktop/src/features/library/types.ts",
  "apps/desktop/src/features/notes/types.ts",
];

function checkStruct(rust, tsFields, label) {
  const rule = rust.attrs.renameAll ?? "(none)";
  const tsByName = new Map(tsFields.map((f) => [f.name, f]));
  let local = 0;
  for (const field of rust.fields) {
    const expected = field.attrs.rename ?? applyRule(field.name, rust.attrs.renameAll);
    const ts = tsByName.get(expected);
    const isOption = unwrapOption(field.type) !== null;
    if (!ts) {
      fail(`${label}: missing TS field \`${expected}\` (Rust \`${field.name}\`, rule ${rule})`);
      continue;
    }
    // Contracts use `field?: T | null` or `field: T | null` for Option;
    // a bare required non-nullable TS field must map to non-Option Rust.
    const tsAllowsNull = ts.optional || /(\|\s*(null|undefined)\b)/.test(ts.type);
    if (tsAllowsNull !== isOption) {
      fail(
        `${label}.${expected}: nullability mismatch (TS ${tsAllowsNull ? "nullable" : "required non-null"} vs Rust ${isOption ? "Option" : "required"})`,
      );
      continue;
    }
    const reason = checkKind(field.type, ts.type, `${label}.${expected}`);
    if (reason) {
      fail(reason);
      continue;
    }
    local += 1;
  }
  const rustNames = new Set(
    rust.fields.map((f) => f.attrs.rename ?? applyRule(f.name, rust.attrs.renameAll)),
  );
  for (const ts of tsFields) {
    if (!rustNames.has(ts.name)) {
      fail(`${label}: extra TS field \`${ts.name}\` with no Rust counterpart`);
    }
  }
  ok(`${label}: ${local} fields match (rule ${rule}, ?/|null = Option)`);
}

function checkLiterals(rust, literals, label) {
  const rule = rust.attrs.renameAll ?? "(none)";
  const expected = rust.variants.map((v) => applyRule(v.name, rust.attrs.renameAll));
  const missing = expected.filter((e) => !literals.includes(e));
  const extra = literals.filter((l) => !expected.includes(l));
  if (missing.length === 0 && extra.length === 0) {
    ok(`${label}: literals match (${expected.join(" | ")}, rule ${rule})`);
    return;
  }
  if (missing.length > 0) fail(`${label}: missing literals ${missing.join(", ")}`);
  if (extra.length > 0) fail(`${label}: extra literals ${extra.join(", ")}`);
}

function checkEvent(rust, tsSrc, tsName, label) {
  const clean = stripTsComments(tsSrc);
  const re = new RegExp(`export\\s+type\\s+${tsName}\\s*=([\\s\\S]*?);(?=\\s*(?:export|declare|$))`);
  const m = clean.match(re);
  if (!m) {
    fail(`${label}: TS type \`${tsName}\` not found`);
    return;
  }
  const members = splitTopLevel(m[1], "|")
    .map((s) => s.trim())
    .filter((s) => s !== "");
  const fieldRule = rust.attrs.renameAllFields ?? "(none)";
  let local = 0;
  for (const variant of rust.variants) {
    const member = members.find((seg) => seg.includes(`"${variant.name}"`));
    if (!member) {
      fail(`${label}: missing TS member for Rust variant \`${variant.name}\``);
      continue;
    }
    if (variant.fields === null) {
      if (/\bpayload\b/.test(member)) {
        fail(`${label}.${variant.name}: unit variant must not carry payload`);
        continue;
      }
      local += 1;
      continue;
    }
    const payloadMatch = member.match(/payload\s*:\s*\{/);
    if (!payloadMatch) {
      fail(`${label}.${variant.name}: missing payload object`);
      continue;
    }
    const openIdx = (payloadMatch.index ?? 0) + payloadMatch[0].length - 1;
    const { body } = matchBraces(member, openIdx);
    const tsFields = parseTsFields(body);
    const tsByName = new Map(tsFields.map((f) => [f.name, f]));
    let fieldsOk = true;
    for (const field of variant.fields) {
      const expected = field.attrs.rename ?? applyRule(field.name, rust.attrs.renameAllFields);
      const ts = tsByName.get(expected);
      const isOption = unwrapOption(field.type) !== null;
      if (!ts || ts.optional !== isOption) {
        fail(`${label}.${variant.name}: payload field \`${expected}\` missing/optionality mismatch`);
        fieldsOk = false;
        continue;
      }
      const reason = checkKind(field.type, ts.type, `${label}.${variant.name}.${expected}`);
      if (reason) {
        fail(reason);
        fieldsOk = false;
      }
    }
    if (fieldsOk) local += 1;
  }
  // TS tag check: every member carries the literal tag.
  const untagged = members.filter((seg) => !/type\s*:\s*"/.test(seg));
  if (untagged.length > 0) {
    fail(`${label}: ${untagged.length} member(s) without literal \`type\` tag`);
    return;
  }
  ok(`${label}: ${local}/${rust.variants.length} variants match (tag \`type\`, fields ${fieldRule})`);
}

function main() {
  console.log("contracts check (Rust serde -> TS mirror)\n");
  for (const group of GROUPS) {
    console.log(`[${group.group}]`);
    const tsSrc = read(group.tsFile);
    for (const pair of group.pairs) {
      const rustSrc = read(pair.rustFile ?? group.rustFile);
      const items = parseRustItems(rustSrc);
      const rust = items.find((it) => it.name === pair.rust);
      if (!rust) {
        fail(`${group.tsFile}: Rust item \`${pair.rust}\` not found`);
        continue;
      }
      const label = `${pair.rust} -> ${pair.ts}`;
      if (pair.kind === "struct") {
        const ts = findTsType(stripTsComments(tsSrc), pair.ts);
        if (!ts || ts.kind !== "object") {
          fail(`${label}: TS object type \`${pair.ts}\` not found`);
          continue;
        }
        checkStruct(rust, parseTsFields(ts.body), label);
      } else if (pair.kind === "literals") {
        const ts = findTsType(stripTsComments(tsSrc), pair.ts);
        if (!ts || ts.kind !== "alias") {
          fail(`${label}: TS literal union \`${pair.ts}\` not found`);
          continue;
        }
        checkLiterals(rust, parseStringLiterals(ts.body), label);
      } else if (pair.kind === "event") {
        checkEvent(rust, tsSrc, pair.ts, label);
      }
    }
  }

  console.log("\n[deferred: Ytdl / Library / full-Note DTOs stay hand-written]");
  let deferredCount = 0;
  for (const rel of DEFERRED_SCAN) {
    const src = read(rel);
    const names = [];
    const re = /^export\s+type\s+([A-Za-z0-9_]+)\s*=/gm;
    let m;
    while ((m = re.exec(src)) !== null) names.push(m[1]);
    for (const name of names) {
      deferredCount += 1;
      warn(`deferred (not in contracts): ${rel} :: ${name}`);
    }
  }

  console.log(`\ncontracts check: ${passed} passed, ${issues.length} failed, ${deferredCount} deferred`);
  if (issues.length > 0) {
    console.log("\nFailures:");
    for (const issue of issues) console.log(`  - ${issue}`);
    process.exit(1);
  }
}

main();
