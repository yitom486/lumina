import { spawn, spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const desktop = path.join(root, "apps", "desktop");
const tauriBin = path.join(
  desktop,
  "node_modules",
  ".bin",
  process.platform === "win32" ? "tauri.exe" : "tauri",
);
const tauriArgs = process.argv.slice(2);
if (tauriArgs.length === 0) tauriArgs.push("dev");

// The Tauri CLI starts both Cargo and the beforeDevCommand (Vite). On Windows,
// Ctrl+C can stop this wrapper without propagating to those grandchildren.
// Keep the Tauri process as the single child we own, then terminate its whole
// process tree so Vite cannot be left behind holding port 1420.
const child = spawn(tauriBin, tauriArgs, {
  cwd: desktop,
  env: process.env,
  stdio: "inherit",
  windowsHide: false,
  detached: process.platform !== "win32",
});

let stopping = false;

function stopChild() {
  if (stopping || child.exitCode !== null || child.pid == null) return;
  stopping = true;

  if (process.platform === "win32") {
    spawnSync(
      "taskkill",
      ["/pid", String(child.pid), "/t", "/f"],
      { stdio: "ignore", windowsHide: true },
    );
    return;
  }

  try {
    process.kill(-child.pid, "SIGTERM");
  } catch {
    child.kill("SIGTERM");
  }
}

process.on("SIGINT", stopChild);
process.on("SIGTERM", stopChild);
process.on("SIGHUP", stopChild);

child.on("error", (error) => {
  console.error(`[tauri-dev] unable to start Tauri: ${error.message}`);
  process.exitCode = 1;
});

child.on("exit", (code, signal) => {
  stopping = true;
  process.exitCode = signal ? 1 : code ?? 1;
});
