import { spawnSync } from "node:child_process";

// Runs the release's setup program and waits for it. The installer asks for
// the folder and offers to open Agent HUD when it finishes.
export async function installWindows({ installer, log }) {
  log("Opening the installer. This test build is not signed yet.");
  const result = spawnSync(installer, [], { stdio: "inherit" });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(`The installer exited with code ${result.status}`);
  }
  log("Done. Run npx agent-hud again to update.");
}
