import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { runInNewContext } from "node:vm";

test("picture documents retain dark tokens without a native appearance bootstrap", () => {
  const html = readFileSync(new URL("../index.html", import.meta.url), "utf8");
  const inline = html.match(/<script>([\s\S]*?)<\/script>/)?.[1];
  assert.ok(
    inline,
    "the document must identify its surface before page modules",
  );
  const classes = new Set();
  const root = {
    dataset: {},
    classList: { add: (name) => classes.add(name) },
  };
  runInNewContext(inline, {
    document: { documentElement: root },
    location: { search: "?surface=picture&pictureId=test" },
    URLSearchParams,
    matchMedia: () => ({ matches: true }),
  });
  assert.equal(root.dataset.startupSurface, "picture");
  assert.equal(root.dataset.theme, "dark");
  assert.equal(classes.has("dark"), true);
});
