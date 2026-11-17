#!/usr/bin/env node
// Packs the browser extension for the Chrome Web Store: a zip of
// src/apps/browser-extension without its manifest's `key`, which the store
// refuses (the key fixes the id of the extension loaded from its folder;
// the store gives its own). Written to build/browser-extension/.
//
//   node src/tooling/pack-browser-extension.mjs
import {
  cpSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { execFileSync } from "node:child_process";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(fileURLToPath(new URL(".", import.meta.url)), "..", "..");
const source = join(root, "src", "apps", "browser-extension");
const out = join(root, "build", "browser-extension");
const stage = join(out, "stage");
rmSync(stage, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
cpSync(source, stage, {
  recursive: true,
  filter: (path) => !path.endsWith("README.md"),
});
const manifest = JSON.parse(readFileSync(join(stage, "manifest.json"), "utf8"));
delete manifest.key;
writeFileSync(
  join(stage, "manifest.json"),
  `${JSON.stringify(manifest, null, 2)}\n`,
);
const zip = join(out, `agent-hud-${manifest.version}.zip`);
rmSync(zip, { force: true });
execFileSync("zip", ["-qr", zip, "."], { cwd: stage });
rmSync(stage, { recursive: true, force: true });
console.log(zip);
