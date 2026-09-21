import test from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const scriptsDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptsDir, "..");
const scriptPath = path.join(scriptsDir, "chapter-agent-e2e-fixture.mjs");
const scriptUrl = pathToFileURL(scriptPath).href;

function runCli(args, env = process.env) {
  const result = spawnSync(process.execPath, [scriptPath, ...args], {
    cwd: repoRoot,
    env,
    encoding: "utf8",
    windowsHide: true,
  });
  assert.equal(result.error, undefined, result.error?.message);
  return result;
}

function childEnv(overrides = {}) {
  return { ...process.env, ...overrides };
}

function runWithMissingTool(tool, args, env = process.env) {
  const wrapper = `
import fs from "node:fs";
import childProcess from "node:child_process";
import { syncBuiltinESMExports } from "node:module";

const originalExistsSync = fs.existsSync.bind(fs);
const originalSpawnSync = childProcess.spawnSync.bind(childProcess);
fs.existsSync = (value) => {
  if (typeof value === "string" && /(?:^|[\\\\/])${tool}(?:\\.exe)?$/i.test(value)) return false;
  return originalExistsSync(value);
};
childProcess.spawnSync = (...spawnArgs) => {
  if (spawnArgs[0] === "where.exe" || spawnArgs[0] === "which") {
    return { status: 1, stdout: "", stderr: "" };
  }
  return originalSpawnSync(...spawnArgs);
};
syncBuiltinESMExports();
process.argv = [process.argv[0], ${JSON.stringify(scriptPath)}, ...${JSON.stringify(args)}];
await import(${JSON.stringify(scriptUrl)});
`;
  const result = spawnSync(
    process.execPath,
    ["--input-type=module", "-e", wrapper],
    { cwd: repoRoot, env, encoding: "utf8", windowsHide: true },
  );
  assert.equal(result.error, undefined, result.error?.message);
  return result;
}

test("fixture CLI --help is deterministic", () => {
  const result = runCli(["--help"]);

  assert.equal(result.status, 0);
  assert.equal(result.stderr, "");
  assert.match(result.stdout, /^Usage:/m);
  assert.match(result.stdout, /--ffmpeg <path>/);
  assert.match(result.stdout, /--ffprobe <path>/);
  assert.match(result.stdout, /--verify <file>/);
});

test("fixture CLI reports unknown arguments without side effects", () => {
  const result = runCli(["--unknown"]);

  assert.equal(result.status, 1);
  assert.equal(result.stdout, "");
  assert.equal(result.stderr, "[chapter-agent-e2e-fixture] Unknown option: --unknown\n");
});

test("fixture CLI gives an actionable ffprobe-unavailable diagnostic", () => {
  const result = runWithMissingTool(
    "ffprobe",
    [],
    childEnv({
      LUMINA_FFPROBE: path.join(os.tmpdir(), "lumina-test-missing-ffprobe.exe"),
      LUMINA_FFMPEG: path.join(os.tmpdir(), "lumina-test-missing-ffmpeg.exe"),
    }),
  );

  assert.equal(result.status, 1);
  assert.equal(result.stdout, "");
  assert.match(result.stderr, /ffprobe is unavailable;.*pass --ffprobe/s);
});

test("fixture CLI gives an actionable ffmpeg-unavailable diagnostic", () => {
  const output = fs.mkdtempSync(path.join(os.tmpdir(), "lumina-chapter-cli-test-"));
  try {
    const result = runWithMissingTool(
      "ffmpeg",
      ["--out", output, "--ffprobe", process.execPath],
      childEnv({ LUMINA_FFMPEG: path.join(output, "missing-ffmpeg.exe") }),
    );

    assert.equal(result.status, 1);
    assert.equal(result.stdout, "");
    assert.match(result.stderr, /ffmpeg is unavailable;.*pass --ffmpeg/s);
  } finally {
    fs.rmSync(output, { recursive: true, force: true });
  }
});
