#!/usr/bin/env node
// Keeps Rust tests passing on Windows as well as on the Mac they are written
// on. Tests in shared files run on every OS (check-platform-boundary keeps OS
// conditions out of them), so two habits that only work on Unix are refused:
//
//   unix-path  a path written from the Unix root: Path::new("/c"),
//              PathBuf::from("/u"), or a folder variable such as
//              ("CLAUDE_CONFIG_DIR", "/c"). On Windows "/c" is not absolute,
//              so code that checks is_absolute() ignores it. Build test paths
//              from tempfile::tempdir() or std::env::temp_dir() instead.
//   plain-dir  fs::create_dir / create_dir_all in tests beside code that only
//              accepts the user's own folders (private_directory, owner
//              checks). An elevated Windows process, as on CI, makes plain
//              folders owned by Administrators, and that code refuses them.
//              Make them with crate::os::private_directory, as the app does.
//              A folder that need not be private (a client's own config
//              folder) may stay plain: end its line with `// plain directory`.
//
// Lines already there are counted per file in test-path-literals.json; only
// new ones fail. Fixing old ones is welcome: run --update to lower the counts.
//
//   node src/tooling/check-test-paths.mjs           fail on anything new
//   node src/tooling/check-test-paths.mjs --update  write today's counts
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(fileURLToPath(new URL(".", import.meta.url)), "..", "..");
const baselinePath = join(root, "src", "tooling", "test-path-literals.json");
const scanned = [join(root, "src", "crates"), join(root, "src", "apps")];
const skipped = new Set(["node_modules", "target", "dist", "gen", "vendor"]);
const hosts = ["macos", "windows", "linux", "unix"];
const testFile = /(^|\/)tests\/|(^|\/|_)tests?\.rs$/;

const rules = {
  "unix-path": [
    /\b(Path::new|PathBuf::from)\(\s*"\//,
    /"[A-Z][A-Z0-9_]*(DIR|HOME|ROOT|PATH|CONFIG)"(\.(to_string|to_owned|into)\(\))?\s*,\s*"\//,
  ],
  "plain-dir": [/\bfs::create_dir(_all)?\(/],
};
const plainMarker = /\/\/\s*plain directory\b/;
const ownerChecks =
  /\b(private_directory|owned_by_user|owner_only|owned_path|private_file|write_private)\b/;

function* rustFiles(directory) {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    if (skipped.has(entry.name)) continue;
    const path = join(directory, entry.name);
    if (entry.isDirectory()) yield* rustFiles(path);
    else if (entry.name.endsWith(".rs")) yield path;
  }
}

const name = (path) => relative(root, path).split(sep).join("/");
const hostSpecific = (path) =>
  name(path)
    .split("/")
    .some((part) => {
      const stem = part.replace(/\.rs$/, "");
      return hosts.some((host) => stem === host || stem.endsWith(`_${host}`));
    });

/** The test part of a file: all of a tests file, else from #[cfg(test)] on. */
function testLines(path, text) {
  const lines = text.split("\n");
  if (testFile.test(name(path))) return lines.map((line, i) => [i, line]);
  const start = lines.findIndex((line) => /^\s*#\[cfg\(test\)\]/.test(line));
  return start < 0
    ? []
    : lines.slice(start).map((line, i) => [start + i, line]);
}

/** Files whose own code checks that a folder is the user's, and module
 * folders (those with a mod.rs) where any file does. */
const checking = new Set();
const modules = new Set();
const sources = new Map();
for (const directory of scanned) {
  for (const path of rustFiles(directory)) {
    if (hostSpecific(path)) continue;
    const text = readFileSync(path, "utf8");
    sources.set(path, text);
    if (path.endsWith(`${sep}mod.rs`)) modules.add(dirname(path));
    if (testFile.test(name(path))) continue;
    const tests = testLines(path, text);
    const code = text.split("\n").slice(0, tests[0]?.[0]).join("\n");
    if (ownerChecks.test(code)) checking.add(path);
  }
}
const moduleChecks = (folder) =>
  modules.has(folder) && [...checking].some((path) => dirname(path) === folder);
/** Whether a file's tests sit beside code that checks folder ownership. */
function guarded(path) {
  if (checking.has(path)) return true;
  const file = name(path);
  const folder = /(^|\/)tests\//.test(file)
    ? dirname(
        path.slice(
          0,
          path.lastIndexOf(`${sep}tests${sep}`) + 1 + "tests".length,
        ),
      )
    : dirname(path);
  return moduleChecks(folder);
}

const counts = { "unix-path": {}, "plain-dir": {} };
const found = { "unix-path": {}, "plain-dir": {} };
for (const [path, text] of sources) {
  const file = name(path);
  for (const [index, line] of testLines(path, text)) {
    if (/^\s*\/\//.test(line)) continue;
    for (const [rule, patterns] of Object.entries(rules)) {
      if (!patterns.some((pattern) => pattern.test(line))) continue;
      if (rule === "plain-dir") {
        if (!guarded(path) || plainMarker.test(line)) continue;
      }
      counts[rule][file] = (counts[rule][file] ?? 0) + 1;
      (found[rule][file] ??= []).push(`${file}:${index + 1}: ${line.trim()}`);
    }
  }
}

if (process.argv.includes("--update")) {
  const sorted = (object) =>
    Object.fromEntries(
      Object.entries(object).sort(([a], [b]) => (a < b ? -1 : 1)),
    );
  writeFileSync(
    baselinePath,
    `${JSON.stringify({ "unix-path": sorted(counts["unix-path"]), "plain-dir": sorted(counts["plain-dir"]) }, null, 2)}\n`,
  );
  console.log(`Wrote ${relative(root, baselinePath)}.`);
  process.exit(0);
}

const baseline = JSON.parse(readFileSync(baselinePath, "utf8"));
const advice = {
  "unix-path":
    "builds a test path from the Unix root, which Windows does not treat as absolute. Use a path under tempfile::tempdir() or std::env::temp_dir().",
  "plain-dir":
    "makes a plain folder beside code that only accepts the user's own folders; on an elevated Windows runner it belongs to Administrators. Use crate::os::private_directory, or end the line with `// plain directory` if the folder need not be private.",
};
let failed = false;
let lower = false;
for (const rule of Object.keys(rules)) {
  for (const [file, count] of Object.entries(counts[rule])) {
    const allowed = baseline[rule]?.[file] ?? 0;
    if (count > allowed) {
      failed = true;
      console.error(
        `${rule}: ${file} has ${count}, ${allowed} allowed. Each line below ${advice[rule]}`,
      );
      for (const line of found[rule][file]) console.error(`  ${line}`);
    }
  }
  for (const [file, allowed] of Object.entries(baseline[rule] ?? {})) {
    if ((counts[rule][file] ?? 0) < allowed) lower = true;
  }
}
if (failed) process.exit(1);
if (lower)
  console.log(
    "Fewer Unix-only test lines than the baseline: run with --update to lock that in.",
  );
console.log("Test paths hold on every OS.");
