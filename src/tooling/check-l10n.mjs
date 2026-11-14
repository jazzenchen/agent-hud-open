#!/usr/bin/env node
// Checks the interface text against the catalogs in
// src/packages/protocol/locales. Text is written in English in the code:
// `t("…")` / `translate(language, "…")` in TypeScript, `tr!("…")` /
// `tr("…")` / `tr_in(language, "…")` in Rust. Every key must be a string
// literal, and every catalog must hold every key (a counted key as
// `<key>_other` at least). Chinese left in source outside comments and
// tests is listed as text not yet moved into the catalogs.
//
//   node src/tooling/check-l10n.mjs            fail on any gap
//   node src/tooling/check-l10n.mjs --report   list gaps, never fail
//   node src/tooling/check-l10n.mjs --keys     print the keys as JSON
import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(fileURLToPath(new URL(".", import.meta.url)), "..", "..");
const locales = join(root, "src", "packages", "protocol", "locales");
const skipped = new Set(["node_modules", "dist", "target", "locales"]);
const report = process.argv.includes("--report");
const printKeys = process.argv.includes("--keys");

function* files(directory) {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    if (skipped.has(entry.name)) continue;
    const path = join(directory, entry.name);
    if (entry.isDirectory()) yield* files(path);
    else if (/\.(ts|tsx|rs)$/.test(entry.name)) yield path;
  }
}

const literal = String.raw`"((?:[^"\\\n]|\\.)*)"`;
const patterns = {
  ts: [
    new RegExp(String.raw`(?<![\w.$])t\(\s*` + literal, "g"),
    new RegExp(String.raw`\btranslate\(\s*[^,()]+,\s*` + literal, "g"),
  ],
  rs: [
    new RegExp(String.raw`\btr!\(\s*(?:count\s*=\s*[^,]+,\s*)?` + literal, "g"),
    new RegExp(String.raw`\btr\(\s*` + literal, "g"),
    new RegExp(String.raw`\btr_(?:count_)?in\(\s*[^,()]+,\s*` + literal, "g"),
  ],
};
const unescape = (text) => JSON.parse(`"${text}"`);
const isTest = (path) =>
  /(^|\/)(tests?|fixtures)\/|_tests?(_\w+)?\.rs$|\/tests\.rs$|\.test\.tsx?$/.test(
    path,
  );

const keys = new Map(); // key → first place
const leftovers = [];
for (const directory of ["src/packages", "src/apps", "src/crates"]) {
  for (const path of files(join(root, directory))) {
    const where = relative(root, path);
    const text = readFileSync(path, "utf8");
    const kind = path.endsWith(".rs") ? "rs" : "ts";
    // Comments are blanked (keeping lines), so examples in them don't count.
    const code = text.replace(
      /\/\*[\s\S]*?\*\/|(?<![:\w])\/\/.*$/gm,
      (comment) => comment.replace(/[^\n]/g, " "),
    );
    // Keys in tests are what tests try, not text the app shows.
    const inlineTests = code.search(/^#\[cfg\(test\)\]\s*\n\s*mod \w+\s*\{/m);
    const shown = isTest(where)
      ? ""
      : inlineTests < 0
        ? code
        : code.slice(0, inlineTests);
    for (const pattern of patterns[kind])
      for (const match of shown.matchAll(pattern)) {
        const key = unescape(match[1]);
        if (!keys.has(key))
          keys.set(
            key,
            `${where}:${code.slice(0, match.index).split("\n").length}`,
          );
      }
    if (isTest(where) || where.includes("/demo/")) continue;
    // An inline test module, which runs to the end of the file.
    const testsAt = text.search(/^#\[cfg\(test\)\]\s*\n\s*mod \w+\s*\{/m);
    const lines = text.split("\n");
    const body = testsAt < 0 ? code : code.slice(0, testsAt);
    body.split("\n").forEach((stripped, index) => {
      const line = lines[index];
      if (/[一-鿿]/.test(stripped) && !/l10n-ignore/.test(line))
        leftovers.push(`${where}:${index + 1}: ${line.trim().slice(0, 100)}`);
    });
  }
}

if (printKeys) {
  console.log(JSON.stringify(Object.fromEntries(keys), null, 1));
  process.exit(0);
}

const problems = [];
const forms = (key) => key.replace(/_(zero|one|two|few|many|other)$/, "");
for (const name of readdirSync(locales).filter((n) => n.endsWith(".json"))) {
  const language = name.slice(0, -5);
  const catalog = JSON.parse(readFileSync(join(locales, name), "utf8"));
  if (language === "en") continue;
  const missing = [...keys.keys()].filter(
    (key) => !(key in catalog) && !(`${key}_other` in catalog),
  );
  const stale = Object.keys(catalog).filter((key) => !keys.has(forms(key)));
  if (missing.length)
    problems.push(
      `${language}: ${missing.length} keys missing, such as\n` +
        missing
          .slice(0, 8)
          .map((key) => `  ${JSON.stringify(key)} (${keys.get(key)})`)
          .join("\n"),
    );
  if (stale.length)
    problems.push(
      `${language}: ${stale.length} keys no code uses, such as\n` +
        stale
          .slice(0, 8)
          .map((key) => `  ${JSON.stringify(key)}`)
          .join("\n"),
    );
}
if (leftovers.length)
  problems.push(
    `${leftovers.length} lines of Chinese left in source:\n` +
      leftovers.slice(0, report ? 400 : 20).join("\n"),
  );

console.log(`${keys.size} keys in code.`);
if (problems.length) {
  console.error(problems.join("\n\n"));
  if (!report) process.exit(1);
}
