import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { download, fetchText } from "./download.mjs";
import * as linux from "./install/linux.mjs";
import * as macos from "./install/macos.mjs";
import { installWindows } from "./install/windows.mjs";
import { HELP, parseOptions } from "./options.mjs";
import { chooseRelease, fetchReleases, parseSums } from "./releases.mjs";
import { currentTarget, describeTarget } from "./target.mjs";

const log = (line) => console.log(line);

export async function main(argv) {
  const options = parseOptions(argv);
  if (options.help) return log(HELP);
  if (options.version) return log(installerVersion());

  const target = currentTarget();
  if (!target) {
    throw new Error(
      `Agent HUD has no build for ${process.platform} on ${process.arch}`,
    );
  }
  const release = chooseRelease(await fetchReleases(), target, options.release);
  if (!release) {
    throw new Error(
      options.release
        ? `Release ${options.release} has no build for ${describeTarget(target)}`
        : `No release has a build for ${describeTarget(target)} yet`,
    );
  }
  const beta = release.prerelease ? " (beta)" : "";
  log(`Agent HUD ${release.version}${beta} for ${describeTarget(target)}`);

  const work = mkdtempSync(join(tmpdir(), "agent-hud-"));
  try {
    const file = join(work, release.asset.name);
    const expected = parseSums(
      await fetchText(release.sums.browser_download_url),
    ).get(release.asset.name);
    if (!expected) {
      throw new Error(`SHA256SUMS has no line for ${release.asset.name}`);
    }
    const actual = await download(
      release.asset.browser_download_url,
      file,
      progress(),
    );
    if (process.stdout.isTTY) process.stdout.write("\n");
    if (actual !== expected) {
      throw new Error(
        `${release.asset.name} does not match its SHA256SUMS line; nothing was installed`,
      );
    }
    await install(target, file, work, options);
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
}

function install(target, file, work, options) {
  switch (target.os) {
    case "macos":
      return macos.installMacos({
        archive: file,
        work,
        folder: options.dir ?? macos.defaultFolder(),
        open: options.open,
        log,
      });
    case "windows":
      if (options.dir) log("--dir is ignored on Windows: the installer asks.");
      return installWindows({ installer: file, log });
    case "linux":
      return linux.installLinux({
        image: file,
        folder: options.dir ?? linux.defaultFolder(),
        open: options.open,
        log,
      });
  }
}

function installerVersion() {
  const manifest = new URL("../package.json", import.meta.url);
  return JSON.parse(readFileSync(manifest, "utf8")).version;
}

// Shows the download in MB on one line while a terminal is watching.
function progress() {
  if (!process.stdout.isTTY) return () => {};
  const mb = (bytes) => (bytes / 1_000_000).toFixed(1);
  let shown = -1;
  return (received, total) => {
    const tenth = Math.floor(received / 100_000);
    if (tenth === shown) return;
    shown = tenth;
    const of = total ? ` / ${mb(total)}` : "";
    process.stdout.write(`\rDownloading ${mb(received)}${of} MB`);
  };
}
