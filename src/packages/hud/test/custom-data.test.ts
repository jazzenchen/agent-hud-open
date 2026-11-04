// What a control of an agent's own reads: the collections a repeat goes over,
// the items it picks, its placeholders filled in and its conditions.
import { test } from "node:test";
import assert from "node:assert/strict";
import type { Snapshot } from "@agent-hud/protocol";
import {
  collections,
  fill,
  holds,
  initialState,
  nextOption,
  pick,
  readsTime,
  windowKind,
  type Scope,
} from "../src/panel/custom/custom-data.ts";

const now = Date.parse("2026-11-04T10:00:00Z");
const snapshot = {
  generatedAt: "2026-11-04T10:00:00Z",
  quotas: [
    {
      id: "claude:a:claude-session",
      accountId: "claude:a",
      account: "me@example.com",
      vendor: "claude",
      name: "Current session",
      shortLabel: "5h",
      usedPercent: 12,
      status: "normal",
      resetAt: "2026-11-04T12:14:00Z",
    },
    {
      id: "claude:a:claude-weekly",
      accountId: "claude:a",
      account: "me@example.com",
      vendor: "claude",
      name: "Weekly limit",
      shortLabel: "Weekly",
      usedPercent: 79,
      status: "warning",
      resetAt: "2026-11-09T10:00:00Z",
    },
    {
      id: "codex:b:codex",
      accountId: "codex:b",
      account: "me",
      vendor: "codex",
      name: "Weekly limit",
      shortLabel: "Weekly",
      usedPercent: 91,
      status: "critical",
      resetAt: null,
    },
  ],
  billing: [
    {
      id: "ds:CNY",
      accountId: "ds",
      vendor: "deepseek",
      label: "API balance",
      amount: 8.85,
      currency: "CNY",
      level: "warning",
    },
  ],
  accounts: [
    {
      id: "claude:a",
      provider: "claude",
      label: "Personal",
      name: "Personal",
      planLabel: "Ultra",
    },
  ],
  providers: [
    { id: "claude", name: "Claude Code", detected: true, enabled: true },
    { id: "codex", name: "Codex", detected: true, enabled: true },
    { id: "cursor", name: "Cursor", detected: false, enabled: true },
  ],
  sessions: [
    {
      id: "s1",
      source: "claude",
      title: "Build the dashboard",
      state: "running",
      updatedAt: "2026-11-04T09:58:00Z",
      lastMessageAt: "2026-11-04T09:58:00Z",
      tokens: 1_200_000,
    },
    {
      id: "s2",
      source: "claude-code",
      title: "Fix the tests",
      state: "idle",
      updatedAt: "2026-11-04T08:00:00Z",
      tokens: 4_000,
    },
    {
      id: "s3",
      source: "codex",
      title: "Review",
      state: "running",
      updatedAt: "2026-11-04T09:00:00Z",
      tokens: 50,
    },
  ],
} as unknown as Snapshot;
const all = collections(snapshot);
const scope = (extra: Partial<Scope> = {}): Scope => ({
  items: [],
  state: {},
  now,
  language: "en",
  ...extra,
});

test("a window's kind is the same on every computer", () => {
  assert.equal(windowKind(snapshot.quotas[0]), "claude-session");
  assert.equal(
    windowKind({
      id: "x",
      accountId: "acct1",
      legacyWindowId: "acct1/claude-weekly",
    } as never),
    "claude-weekly",
  );
  assert.deepEqual(
    all.windows.map((w) => w.used),
    [0.12, 0.79, 0.91],
  );
});

test("agents count their sessions under every name they go by", () => {
  assert.deepEqual(
    all.agents.map((a) => [a.id, a.sessions, a.running]),
    [
      ["claude", 2, 1],
      ["codex", 1, 1],
    ],
  );
});

test("a repeat picks, sorts and limits", () => {
  const tight = pick(
    all,
    "windows",
    { level: ["warning", "critical"] },
    "-used",
    5,
    scope(),
  );
  assert.deepEqual(
    tight.map((w) => w.id),
    ["codex:b:codex", "claude:a:claude-weekly"],
  );
  const recent = pick(all, "sessions", undefined, "-updatedAt", 2, scope());
  assert.deepEqual(
    recent.map((s) => s.id),
    ["s1", "s3"],
  );
  // An agent matches by its mark, and a state can choose it.
  const chosen = pick(
    all,
    "windows",
    { agent: "{state.agent}" },
    undefined,
    4,
    scope({ state: { agent: "claude-code" } }),
  );
  assert.equal(chosen.length, 2);
});

test("placeholders read the item around them in its own format", () => {
  const [window] = pick(
    all,
    "windows",
    { kind: "claude-session" },
    undefined,
    1,
    scope(),
  );
  const inside = scope({ items: [{ name: "window", item: window }] });
  assert.equal(fill("{window.short} {window.used}", inside), "5h 12%");
  assert.equal(fill("resets in {window.resetsAt}", inside), "resets in 2h 14m");
  assert.equal(fill("{window.used|number}", inside), "0.12");
  const [balance] = all.balances;
  assert.equal(
    fill(
      "{balance.amount}",
      scope({ items: [{ name: "balance", item: balance }] }),
    ),
    "¥8.85",
  );
  const [session] = all.sessions;
  assert.equal(
    fill(
      "{session.title} · {session.tokens} · {session.updatedAt}",
      scope({ items: [{ name: "session", item: session }] }),
    ),
    "Build the dashboard · 1.2M · 2m ago",
  );
  assert.equal(fill("{nothing.here}", scope()), "—");
});

test("conditions test a value where it stands", () => {
  const [, weekly] = all.windows;
  const inside = scope({
    items: [{ name: "window", item: weekly }],
    state: { tab: "week" },
  });
  assert.equal(holds({ value: "{window.used}", above: 0.7 }, inside), true);
  assert.equal(
    holds({ value: "{window.level}", in: ["critical"] }, inside),
    false,
  );
  assert.equal(holds({ value: "{state.tab}", is: "week" }, inside), true);
  assert.equal(
    holds({ value: "{window.runsOutAt}", present: true }, inside),
    false,
  );
});

test("states start at their defaults and step round", () => {
  const node = {
    state: { tab: { options: ["day", "week", "month"], default: "week" } },
  };
  assert.deepEqual(initialState(node), { tab: "week" });
  assert.equal(nextOption(node, "tab", "month"), "day");
  assert.equal(readsTime({ type: "text", text: "{now.time|clock}" }), true);
  assert.equal(readsTime({ type: "text", text: "{window.used}" }), false);
});
