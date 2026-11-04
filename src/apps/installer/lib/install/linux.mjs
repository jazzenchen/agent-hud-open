import { spawn } from "node:child_process";
import {
  chmodSync,
  copyFileSync,
  mkdirSync,
  renameSync,
  rmSync,
} from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

const IMAGE = "Agent-HUD.AppImage";

export const defaultFolder = () => join(homedir(), "Applications");

// Puts the AppImage at one fixed path: Agent HUD points the agents' hooks at
// the image file, so an update must land where the last one was. A running
// copy keeps its old file open and picks up the new one when it restarts.
export async function installLinux({ image, folder, open, log }) {
  mkdirSync(folder, { recursive: true });
  const destination = join(folder, IMAGE);
  const staged = join(folder, `.${IMAGE}.${process.pid}`);
  try {
    copyFileSync(image, staged);
    chmodSync(staged, 0o755);
    renameSync(staged, destination);
  } finally {
    rmSync(staged, { force: true });
  }
  log(`Installed ${destination}`);
  log(
    "This test build is not signed yet. Run npx @agenthud/install again to update.",
  );
  if (open) spawn(destination, [], { detached: true, stdio: "ignore" }).unref();
}
