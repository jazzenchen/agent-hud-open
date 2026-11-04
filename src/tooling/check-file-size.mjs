#!/usr/bin/env node
// Keeps source files small enough to read: no file's own code runs past 300
// lines. Split a file that grows past it along its seams (components, hooks,
// impl blocks, per-OS parts) rather than packing lines.
//
// Counted: .rs, .ts, .tsx and .mjs under src/crates, src/apps and
// src/packages. Not counted: test files (tests.rs, *_tests.rs, tests/,
// test/, *.test.ts, *.test.mjs) and a Rust file's own test module, from its
// `#[cfg(test)] mod … {` on.
//
// Files already over the limit are listed with their size in
// file-size-allowances.json; they may shrink but not grow, and new files may
// not pass the limit. A file that stays over has a reason: one state machine,
// one timeline, one data table. After shrinking one, run --update.
//
//   node src/tooling/check-file-size.mjs           fail on anything new
//   node src/tooling/check-file-size.mjs --update  write today's sizes
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";

const limit = 300;
const root = join(fileURLToPath(new URL(".", import.meta.url)), "..", "..");
const allowancesPath = join(
  root,
  "src",
  "tooling",
  "file-size-allowances.json",
);
const scanned = ["crates", "apps", "packages"].map((part) =>
  join(root, "src", part),
);
const skipped = new Set([
  "node_modules",
  "target",
  "dist",
  "gen",
  "vendor",
  "tests",
  "test",
]);
const source = /\.(rs|ts|tsx|mjs)$/;
const testFile = /(^|_)tests?(_\w+)?\.rs$|\.test\.(ts|mjs)$|\.d\.ts$/;

function* sourceFiles(directory) {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    if (skipped.has(entry.name)) continue;
    const path = join(directory, entry.name);
    if (entry.isDirectory()) yield* sourceFiles(path);
    else if (source.test(entry.name) && !testFile.test(entry.name)) yield path;
  }
}

// The file's own lines: a Rust file stops where its inline test module starts.
function ownLines(path) {
  const lines = readFileSync(path, "utf8").split("\n");
  if (lines.at(-1) === "") lines.pop();
  if (!path.endsWith(".rs")) return lines.length;
  for (let index = 0; index < lines.length - 1; index++) {
    if (
      lines[index].trim() === "#[cfg(test)]" &&
      /^mod \w+ \{/.test(lines[index + 1])
    )
      return index;
  }
  return lines.length;
}

const sizes = {};
for (const directory of scanned) {
  for (const path of sourceFiles(directory)) {
    const size = ownLines(path);
    if (size > limit) sizes[relative(root, path).split(sep).join("/")] = size;
  }
}

if (process.argv.includes("--update")) {
  const sorted = Object.fromEntries(
    Object.entries(sizes).sort(([a], [b]) => (a < b ? -1 : 1)),
  );
  writeFileSync(allowancesPath, `${JSON.stringify(sorted, null, 2)}\n`);
  console.log(`Wrote ${relative(root, allowancesPath)}.`);
  process.exit(0);
}

const allowances = JSON.parse(readFileSync(allowancesPath, "utf8"));
let failed = false;
for (const [file, size] of Object.entries(sizes)) {
  const allowed = allowances[file] ?? limit;
  if (size > allowed) {
    failed = true;
    console.error(
      allowances[file]
        ? `${file} has ${size} lines of its own, up from ${allowed}. Split it rather than grow it.`
        : `${file} has ${size} lines of its own, over ${limit}. Split it along its seams.`,
    );
  }
}
if (failed) process.exit(1);
if (
  Object.entries(allowances).some(([file, size]) => (sizes[file] ?? 0) < size)
)
  console.log(
    "Some listed files are smaller now: run with --update to lock that in.",
  );
console.log(`No source file grew past ${limit} lines.`);
