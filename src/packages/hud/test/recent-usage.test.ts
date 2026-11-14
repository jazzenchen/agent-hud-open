import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { addCatalog } from "@agent-hud/ui/format";
import { recentCost, recentLabel, recentUsage } from "../src/recent-usage.ts";

// The Chinese below is zh-Hans.json's.
const locales = join(import.meta.dirname, "../../protocol/locales");
for (const language of ["en", "zh-Hans"] as const)
  addCatalog(
    language,
    JSON.parse(readFileSync(join(locales, `${language}.json`), "utf8")),
  );

const provider = (
  id: string,
  recent: {
    tokens: number;
    costs: Record<string, number>;
    unpricedTokens: number;
  } | null,
) => ({
  id,
  name: id,
  detected: true,
  enabled: true,
  notice: null,
  sessions: 0,
  recent,
});

const snapshot = {
  providers: [
    provider("hermes", {
      tokens: 1_260_000,
      costs: {},
      unpricedTokens: 1_260_000,
    }),
    provider("claude", {
      tokens: 5_000,
      costs: { USD: 1.2 },
      unpricedTokens: 0,
    }),
    provider("codex", {
      tokens: 9_000,
      costs: { USD: 412.5 },
      unpricedTokens: 300,
    }),
    provider("kimi", {
      tokens: 9_000,
      costs: { CNY: 2, USD: 0.5 },
      unpricedTokens: 0,
    }),
    provider("pi", null),
    provider("opencode", { tokens: 700, costs: { USD: 0 }, unpricedTokens: 0 }),
  ],
} as never;

const label = (key: string) => recentLabel(recentUsage(snapshot, key), "en");

test("an agent with no window or balance shows its last 24 hours, as vendors.rs writes them", () => {
  assert.equal(label("hermes"), "1.3M", "no price: its tokens");
  assert.equal(label("claude"), "≈$1.20", "priced: its cost");
  assert.equal(
    label("openai"),
    "≈$412+",
    "Codex under the OpenAI mark, partly priced: the priced part, then +",
  );
  assert.equal(
    label("kimi"),
    "≈$0.50+",
    "two currencies: US dollars first, then + for the other",
  );
  assert.equal(label("pi"), null, "nothing used: nothing to show");
  assert.equal(label("opencode"), "Free", "a local model costs nothing");
  assert.equal(
    recentLabel(recentUsage(snapshot, "opencode"), "zh-Hans"),
    "免费",
  );
});

test("recent usage sums the providers behind one mark", () => {
  const twice = {
    providers: [
      provider("codex", { tokens: 100, costs: { USD: 1 }, unpricedTokens: 0 }),
      provider("chatgpt", {
        tokens: 50,
        costs: { USD: 2 },
        unpricedTokens: 10,
      }),
    ],
  } as never;
  assert.deepEqual(recentUsage(twice, "openai"), {
    tokens: 150,
    costs: { USD: 3 },
    unpricedTokens: 10,
  });
  assert.equal(
    recentCost({ tokens: 10, costs: {}, unpricedTokens: 10 }, "en"),
    null,
  );
});
