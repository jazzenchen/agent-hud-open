import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { runInNewContext } from "node:vm";

test("the document identifies its surface before page modules load", () => {
  const html = readFileSync(new URL("../index.html", import.meta.url), "utf8");
  const inline = html.match(/<script>([\s\S]*?)<\/script>/)?.[1];
  assert.ok(
    inline,
    "the document must identify its surface before page modules",
  );
  for (const [search, surface] of [
    ["", "settings"],
    ["?window=statistics", "statistics"],
    ["?surface=hud&screen=test", "hud"],
    ["?surface=picture&pictureId=test", "picture"],
  ]) {
    const root = { dataset: {} };
    runInNewContext(inline, {
      document: { documentElement: root },
      location: { search },
      URLSearchParams,
    });
    assert.equal(root.dataset.startupSurface, surface);
  }
});
