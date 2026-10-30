import { spawnSync } from "node:child_process";
import { mkdirSync, copyFileSync, chmodSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join, resolve } from "node:path";
const root = fileURLToPath(new URL("../../", import.meta.url));
const triple = process.env.TAURI_ENV_TARGET_TRIPLE;
const platform = triple?.includes("windows")
  ? "windows"
  : triple?.includes("apple")
    ? "darwin"
    : triple?.includes("linux")
      ? "linux"
      : process.platform === "win32"
        ? "windows"
        : process.platform;
const arch = triple?.startsWith("aarch64")
  ? "arm64"
  : triple?.startsWith("x86_64")
    ? "x64"
    : process.arch;
const extension = platform === "windows" ? ".exe" : "";
// A product built on this repository passes its own folder with --out.
const out = process.argv.indexOf("--out");
const output =
  out > 0 && process.argv[out + 1]
    ? resolve(process.argv[out + 1])
    : join(root, "src/apps/desktop/src-tauri/resources");
mkdirSync(output, { recursive: true });
function run(exe, args) {
  const result = spawnSync(exe, args, { cwd: root, stdio: "inherit" });
  if (result.status !== 0)
    throw Error(`${exe} failed (${result.status}): ${result.error ?? ""}`);
}
const debug = process.env.TAURI_ENV_DEBUG === "true";
const targetDirectory = process.env.CARGO_TARGET_DIR
  ? resolve(root, process.env.CARGO_TARGET_DIR)
  : join(root, "target");
run("cargo", [
  "build",
  "-p",
  "agent-hud-engine",
  ...(debug ? [] : ["--release"]),
  ...(triple ? ["--target", triple] : []),
]);
copyFileSync(
  join(
    targetDirectory,
    ...(triple ? [triple] : []),
    debug ? "debug" : "release",
    `agent-hud-engine${extension}`,
  ),
  join(output, `agent-hud-engine${extension}`),
);
run(process.env.BUN_EXECUTABLE ?? "bun", [
  "build",
  "--compile",
  `--target=bun-${platform}-${arch}`,
  "src/apps/mcp/src/index.ts",
  "--outfile",
  join(output, `agent-hud-mcp${extension}`),
]);
if (platform !== "windows") {
  chmodSync(join(output, "agent-hud-engine"), 0o755);
  chmodSync(join(output, "agent-hud-mcp"), 0o755);
}
