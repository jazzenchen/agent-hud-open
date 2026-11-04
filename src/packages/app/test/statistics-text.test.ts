import { test } from "node:test";
import assert from "node:assert/strict";
import { listCost } from "@agent-hud/ui/format";
import {
  sessionClient,
  sessionHelp,
  sessionKinds,
} from "../src/lib/sessions.ts";
import { hourModels } from "../src/lib/statistics.ts";

test("list costs read as Open's ListCost.text: ≈ each currency, US dollars first, then by code", () => {
  assert.equal(listCost({ CNY: 45.6, USD: 12.3 }, "en"), "≈$12.30 · ≈¥45.60");
  assert.equal(
    listCost({ EUR: 2, CNY: 8.85, USD: 1 }, "zh"),
    "≈$1.00 · ≈¥8.85 · ≈€2.00",
  );
  assert.equal(
    listCost({ CNY: 8.85 }, "en"),
    "≈¥8.85",
    "the narrow symbol, never CN¥",
  );
  assert.equal(
    listCost({ USD: 0.004 }, "en"),
    "≈$0.00",
    "two decimals, as Open writes a list cost",
  );
  assert.equal(listCost({}, "en"), "");
});

test("a currency code Intl does not know still reads as an amount", () => {
  assert.equal(listCost({ usd1: 3 }, "en"), "≈usd1 3.00");
});

const providers = [
  { id: "claude", name: "Claude Code" },
  { id: "deepseek", name: "DeepSeek" },
];

test("a session's client is Core's name for it, else its agent's name, else the agent's id", () => {
  assert.equal(
    sessionClient({ source: "codex", client: "Codex Desktop" }, providers),
    "Codex Desktop",
  );
  assert.equal(
    sessionClient(
      { source: "claude", client: "Claude Code IDE extension" },
      providers,
    ),
    "Claude Code IDE extension",
  );
  assert.equal(sessionClient({ source: "deepseek" }, providers), "DeepSeek");
  assert.equal(
    sessionClient({ source: "deepseek", client: null }, providers),
    "DeepSeek",
  );
  assert.equal(sessionClient({ source: "grok-cli" }, providers), "grok-cli");
  assert.equal(sessionClient({ source: "pi" }), "pi");
});

test("a row's tooltip is the title, the client and the folder", () => {
  assert.equal(
    sessionHelp({ title: "Fix the build", cwd: "/w/app" }, "Codex Desktop"),
    "Fix the build · Codex Desktop · /w/app",
  );
  assert.equal(
    sessionHelp({ title: "", cwd: null }, "Codex Desktop"),
    "Codex Desktop",
  );
});

const usage = (
  kinds: Partial<
    Record<
      "input" | "output" | "reasoning" | "cacheWrite" | "cacheRead",
      number
    >
  >,
) => {
  const full = {
    input: 0,
    output: 0,
    reasoning: 0,
    cacheWrite: 0,
    cacheRead: 0,
    ...kinds,
  };
  return {
    ...full,
    total: Object.values(full).reduce((sum, value) => sum + value, 0),
  };
};

test("the Today card splits its sessions' tokens by every kind present, in Open's order", () => {
  const today = sessionKinds([
    {
      tokens: 1_100,
      usage: usage({ input: 10, output: 20, cacheRead: 1_000, cacheWrite: 70 }),
    },
    { tokens: 50, usage: usage({ input: 5, output: 45, reasoning: 0 }) },
  ]);
  assert.equal(today.total, 1_150);
  assert.deepEqual(
    today.kinds,
    [
      { kind: "cacheWrite", value: 70 },
      { kind: "input", value: 15 },
      { kind: "output", value: 65 },
      { kind: "cacheRead", value: 1_000 },
    ],
    "reasoning is absent, so it is left out",
  );
});

test("without any breakdown the Today card keeps its total and has no kinds", () => {
  assert.deepEqual(sessionKinds([{ tokens: 40 }, { tokens: 2 }]), {
    total: 42,
    kinds: null,
  });
  assert.deepEqual(sessionKinds([]), { total: 0, kinds: null });
  assert.deepEqual(sessionKinds([{ tokens: 0, usage: usage({}) }]), {
    total: 0,
    kinds: [],
  });
  // A row Core has no log for (a hook-only session) adds its tokens, not kinds.
  assert.deepEqual(
    sessionKinds([{ tokens: 30, usage: usage({ input: 30 }) }, { tokens: 0 }]),
    {
      total: 30,
      kinds: [{ kind: "input", value: 30 }],
    },
  );
});

test("an hour's models read most tokens first, ties by id", () => {
  assert.deepEqual(
    hourModels({ "codex:gpt-5": 10, "claude:opus": 30, "claude:haiku": 10 }),
    [
      ["claude:opus", 30],
      ["claude:haiku", 10],
      ["codex:gpt-5", 10],
    ],
  );
  assert.deepEqual(hourModels(undefined), []);
});
