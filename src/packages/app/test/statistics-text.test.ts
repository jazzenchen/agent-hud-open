import { test } from "node:test";
import assert from "node:assert/strict";
import { listCost } from "@agent-hud/ui/format";
import {
  presentKinds,
  sessionClient,
  sessionHelp,
} from "../src/lib/sessions.ts";

test("list costs read as Open's ListCost.text: ≈ each currency, US dollars first, then by code", () => {
  assert.equal(listCost({ CNY: 45.6, USD: 12.3 }, "en"), "≈$12.30 · ≈¥45.60");
  assert.equal(
    listCost({ EUR: 2, CNY: 8.85, USD: 1 }, "zh-Hans"),
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

test("the Today card names every kind present, in Open's order", () => {
  assert.deepEqual(
    presentKinds({
      input: 15,
      output: 65,
      reasoning: 0,
      cacheWrite: 70,
      cacheRead: 1_000,
    }),
    [
      { kind: "cacheWrite", value: 70 },
      { kind: "input", value: 15 },
      { kind: "output", value: 65 },
      { kind: "cacheRead", value: 1_000 },
    ],
    "reasoning is absent, so it is left out",
  );
});
