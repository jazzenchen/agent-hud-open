import { execFileSync, spawnSync } from "node:child_process";
import {
  accessSync,
  constants,
  existsSync,
  mkdirSync,
  renameSync,
  rmSync,
} from "node:fs";
import { join } from "node:path";
import { compareVersions } from "../releases.mjs";

const APP = "Agent HUD.app";
const TEAM = "4H8CWRAN3M";

export const defaultFolder = () => "/Applications";

// Unpacks the release's .app.tar.gz, checks the app, and puts it in place of
// any earlier copy. The folder stays the same across updates on purpose: at
// launch Agent HUD points every agent's hooks at its own path.
export async function installMacos({ archive, work, folder, open, log }) {
  const unpacked = join(work, "unpacked");
  mkdirSync(unpacked);
  run("tar", ["-xzf", archive, "-C", unpacked]);
  const app = join(unpacked, APP);
  if (!existsSync(app)) throw new Error(`The archive holds no ${APP}`);
  checkSignature(app);
  checkSystem(app);

  const destination = join(folder, APP);
  if (sameBuild(app, destination)) {
    log(`${destination} is already this build.`);
  } else {
    writable(folder);
    await quit(destination, log);
    replace(app, destination, folder);
    log(`Installed ${destination}`);
  }
  if (open) run("open", [destination]);
}

function checkSignature(app) {
  run("codesign", ["--verify", "--deep", "--strict", app]);
  const details = spawnSync("codesign", ["-dv", app], { encoding: "utf8" });
  if (!`${details.stderr}`.includes(`TeamIdentifier=${TEAM}`)) {
    throw new Error("The app is not signed by Agent HUD's developer");
  }
}

function checkSystem(app) {
  const needed = plist(app, "LSMinimumSystemVersion");
  const running = execFileSync("sw_vers", ["-productVersion"], {
    encoding: "utf8",
  }).trim();
  if (needed && compareVersions(running, needed) < 0) {
    throw new Error(
      `Agent HUD needs macOS ${needed} or later; this Mac runs ${running}`,
    );
  }
}

function sameBuild(app, destination) {
  if (!existsSync(destination)) return false;
  const keys = ["CFBundleIdentifier", "CFBundleVersion"];
  return keys.every((key) => plist(app, key) === plist(destination, key));
}

function plist(app, key) {
  const result = spawnSync(
    "plutil",
    ["-extract", key, "raw", join(app, "Contents", "Info.plist")],
    { encoding: "utf8" },
  );
  return result.status === 0 ? result.stdout.trim() : null;
}

function writable(folder) {
  mkdirSync(folder, { recursive: true });
  try {
    accessSync(folder, constants.W_OK);
  } catch {
    throw new Error(
      `Can't write to ${folder}. Run again with --dir ~/Applications to install for you alone.`,
    );
  }
}

// Ends the running app from this folder, leaving its hook processes alone:
// one of them may be holding an agent's permission request.
async function quit(destination, log) {
  const executable = join(destination, "Contents", "MacOS", "Agent HUD");
  const listing = execFileSync("ps", ["-axo", "pid=,command="], {
    encoding: "utf8",
  });
  const pids = listing
    .split("\n")
    .map((line) => line.trim().match(/^(\d+)\s+(.*)$/))
    .filter((match) => match && match[2].trim() === executable)
    .map((match) => Number(match[1]));
  if (!pids.length) return;
  log("Quitting Agent HUD to replace it.");
  for (const pid of pids) process.kill(pid, "SIGTERM");
  for (let waited = 0; waited < 10_000; waited += 250) {
    if (!pids.some(alive)) return;
    await new Promise((done) => setTimeout(done, 250));
  }
  throw new Error("Agent HUD did not quit. Quit it and run this again.");
}

function alive(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

// Copies the app beside the old one first, so the swap is two renames and a
// failure leaves the old app where it was.
function replace(app, destination, folder) {
  const staged = join(folder, `.agent-hud-new-${process.pid}`);
  const previous = join(folder, `.agent-hud-old-${process.pid}`);
  rmSync(staged, { recursive: true, force: true });
  run("ditto", [app, staged]);
  try {
    if (existsSync(destination)) renameSync(destination, previous);
    renameSync(staged, destination);
  } catch (error) {
    if (existsSync(previous) && !existsSync(destination)) {
      renameSync(previous, destination);
    }
    rmSync(staged, { recursive: true, force: true });
    if (error.code === "EPERM" || error.code === "EACCES") {
      throw new Error(
        `macOS kept ${destination} from being replaced. Allow your terminal under System Settings > Privacy & Security > App Management, then run this again.`,
      );
    }
    throw error;
  }
  rmSync(previous, { recursive: true, force: true });
}

function run(command, args) {
  const result = spawnSync(command, args, { encoding: "utf8" });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    const detail = `${result.stderr}`.trim().split("\n").pop();
    throw new Error(`${command} failed${detail ? `: ${detail}` : ""}`);
  }
  return result.stdout;
}
