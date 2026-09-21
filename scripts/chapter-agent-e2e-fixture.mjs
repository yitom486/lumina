#!/usr/bin/env node

/**
 * Build and verify a deterministic local input for the Chapter Agent E2E.
 *
 * This script deliberately does not start Tauri, an ACP adapter, or an Agent.
 * It only makes the media/evidence precondition reproducible and proves that
 * the resulting container has a subtitle stream and no container chapters.
 */

import { execFileSync, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const defaultOutput = path.join(repoRoot, ".tmp", "chapter-agent-e2e");
const fixtureName = "chapter-agent-fixture.mkv";
const subtitleName = "chapter-agent-fixture.srt";
const manifestName = "chapter-agent-fixture.manifest.json";

const subtitle = `1
00:00:00,400 --> 00:00:02,200
抵达旧车站，调查从一张未寄出的车票开始。

2
00:00:02,600 --> 00:00:05,400
广播突然中断，镜头转向月台尽头的红灯。

3
00:00:05,800 --> 00:00:08,600
两个人交换线索，但没有说明谁先发现了秘密。

4
00:00:09,000 --> 00:00:11,600
列车驶离后，留下一个需要继续追踪的时间点。
`;

function usage() {
  console.log(`Usage:
  bun scripts/chapter-agent-e2e-fixture.mjs [options]

Options:
  --out <dir>       Output directory (default: ${defaultOutput})
  --ffmpeg <path>   Explicit ffmpeg executable
  --ffprobe <path>  Explicit ffprobe executable
  --verify <file>   Verify an existing media file; do not generate media
  --force           Replace generated files in --out
  --help            Show this help

Environment fallbacks: LUMINA_FFMPEG and LUMINA_FFPROBE, then the project
native/ffmpeg locations and finally PATH.`);
}

function parseArgs(argv) {
  const options = { output: defaultOutput, force: false, verify: null };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--help") {
      usage();
      process.exit(0);
    }
    if (arg === "--force") {
      options.force = true;
      continue;
    }
    if (arg === "--out" || arg === "--ffmpeg" || arg === "--ffprobe" || arg === "--verify") {
      const value = argv[index + 1];
      if (!value || value.startsWith("--")) {
        throw new Error(`${arg} requires a value`);
      }
      index += 1;
      if (arg === "--out") options.output = path.resolve(value);
      if (arg === "--ffmpeg") options.ffmpeg = path.resolve(value);
      if (arg === "--ffprobe") options.ffprobe = path.resolve(value);
      if (arg === "--verify") options.verify = path.resolve(value);
      continue;
    }
    throw new Error(`Unknown option: ${arg}`);
  }
  return options;
}

function existingFile(value) {
  return value && existsSync(value) ? value : null;
}

function findOnPath(command) {
  const lookup = process.platform === "win32" ? "where.exe" : "which";
  const result = spawnSync(lookup, [command], { encoding: "utf8", windowsHide: true });
  if (result.status !== 0) return null;
  return result.stdout.split(/\r?\n/).map((line) => line.trim()).find(Boolean) ?? null;
}

function resolveTool(explicit, envName, unixName, windowsName) {
  const candidates = [
    explicit,
    process.env[envName],
    path.join(repoRoot, "apps", "desktop", "src-tauri", "native", "ffmpeg", windowsName),
    path.join(repoRoot, "native", "ffmpeg", windowsName),
    path.join(repoRoot, "apps", "desktop", "src-tauri", "native", "ffmpeg", unixName),
    path.join(repoRoot, "native", "ffmpeg", unixName),
  ];
  for (const candidate of candidates) {
    const resolved = existingFile(candidate);
    if (resolved) return resolved;
  }
  return findOnPath(unixName);
}

function fail(message) {
  throw new Error(message);
}

function run(command, args, label) {
  try {
    return execFileSync(command, args, {
      cwd: repoRoot,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
      windowsHide: true,
    });
  } catch (error) {
    const stderr = error?.stderr?.toString().trim();
    throw new Error(`${label} failed${stderr ? `: ${stderr}` : ""}`);
  }
}

function probe(ffprobe, mediaPath) {
  const raw = run(
    ffprobe,
    [
      "-v",
      "error",
      "-print_format",
      "json",
      "-show_format",
      "-show_streams",
      "-show_chapters",
      mediaPath,
    ],
    "ffprobe",
  );
  try {
    return JSON.parse(raw);
  } catch {
    fail("ffprobe returned invalid JSON");
  }
}

function verifyMedia(ffprobe, mediaPath) {
  if (!existsSync(mediaPath)) fail(`media file does not exist: ${mediaPath}`);
  const report = probe(ffprobe, mediaPath);
  const streams = Array.isArray(report.streams) ? report.streams : [];
  const subtitleStreams = streams.filter((stream) => stream.codec_type === "subtitle");
  const videoStreams = streams.filter((stream) => stream.codec_type === "video");
  const chapters = Array.isArray(report.chapters) ? report.chapters : [];
  const durationSeconds = Number(report.format?.duration);

  if (videoStreams.length === 0) fail("fixture has no video stream");
  if (subtitleStreams.length === 0) fail("fixture has no embedded subtitle stream");
  if (chapters.length !== 0) fail(`fixture unexpectedly has ${chapters.length} container chapters`);
  if (!Number.isFinite(durationSeconds) || durationSeconds < 10) {
    fail("fixture duration is shorter than the 10 second E2E minimum");
  }

  return {
    media: path.resolve(mediaPath),
    durationMs: Math.round(durationSeconds * 1000),
    videoStreams: videoStreams.length,
    subtitleStreams: subtitleStreams.map((stream) => ({
      index: stream.index,
      codec: stream.codec_name,
      language: stream.tags?.language ?? null,
    })),
    chapters: chapters.length,
  };
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  const ffmpeg = resolveTool(options.ffmpeg, "LUMINA_FFMPEG", "ffmpeg", "ffmpeg.exe");
  const ffprobe = resolveTool(options.ffprobe, "LUMINA_FFPROBE", "ffprobe", "ffprobe.exe");
  if (!ffprobe) fail("ffprobe is unavailable; place it under apps/desktop/src-tauri/native/ffmpeg or pass --ffprobe");

  if (options.verify) {
    console.log(JSON.stringify({ status: "ok", ...verifyMedia(ffprobe, options.verify) }, null, 2));
    return;
  }
  if (!ffmpeg) fail("ffmpeg is unavailable; place it under apps/desktop/src-tauri/native/ffmpeg or pass --ffmpeg");

  mkdirSync(options.output, { recursive: true });
  const mediaPath = path.join(options.output, fixtureName);
  const subtitlePath = path.join(options.output, subtitleName);
  const manifestPath = path.join(options.output, manifestName);
  if (!options.force && [mediaPath, subtitlePath, manifestPath].some((file) => existsSync(file))) {
    fail(`output already exists; use --force only for this generated fixture directory: ${options.output}`);
  }

  writeFileSync(subtitlePath, subtitle, "utf8");
  run(
    ffmpeg,
    [
      "-y",
      "-v",
      "error",
      "-f",
      "lavfi",
      "-i",
      "testsrc2=size=640x360:rate=24",
      "-f",
      "lavfi",
      "-i",
      "sine=frequency=440:sample_rate=48000",
      "-i",
      subtitlePath,
      "-map",
      "0:v:0",
      "-map",
      "1:a:0",
      "-map",
      "2:0",
      "-t",
      "12",
      "-c:v",
      "mpeg4",
      "-q:v",
      "5",
      "-c:a",
      "aac",
      "-c:s",
      "srt",
      "-metadata:s:s:0",
      "language=zho",
      "-map_metadata",
      "-1",
      "-map_chapters",
      "-1",
      mediaPath,
    ],
    "ffmpeg",
  );

  const report = verifyMedia(ffprobe, mediaPath);
  const manifest = {
    schema: "lumina.chapter-agent-e2e-fixture/v1",
    purpose: "local subtitle-backed media without container chapters",
    subtitleCues: 4,
    ...report,
  };
  writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`, "utf8");
  console.log(JSON.stringify({ status: "ok", ffmpeg, ffprobe, ...manifest }, null, 2));
}

try {
  main();
} catch (error) {
  console.error(`[chapter-agent-e2e-fixture] ${error instanceof Error ? error.message : String(error)}`);
  process.exitCode = 1;
}
