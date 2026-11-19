import { test } from "node:test";
import assert from "node:assert/strict";
import type { PanelLayout, Quota, Snapshot } from "@agent-hud/protocol";
import {
  configuredVendors,
  defaultHUDConfig,
  followsShortestWindow,
  meterLabel,
  type HUDConfig,
} from "../src/model.ts";
import { windowReading } from "../src/held-reading.ts";
import { shownRows } from "../src/quota-rows.ts";

// Resets fall hours from now, never on a fixed date: configuredVendors reads
// the clock itself, so a dated reset would pass and turn every window stale.
const now = Date.now();
const hoursFromNow = (hours: number) =>
  new Date(now + hours * 3_600_000).toISOString();
const row = (
  window: string,
  usedPercent: number,
  status: string,
  windowDurationSeconds?: number | null,
  resetAt = hoursFromNow(5),
) =>
  ({
    id: `acct:${window}`,
    vendor: "claude",
    name: window,
    usedPercent,
    resetAt,
    status,
    account: "a@example.com",
    accountId: "acct",
    windowDurationSeconds,
  }) as Quota;
const quotas = [
  row("claude-5h", 80, "normal", 18_000),
  row("claude-7d", 90, "normal", 604_800),
];
// What configuredVendors sets on a mark (model.ts `withShownWindows`).
const reading = (shortestFirst: boolean, blocks?: PanelLayout["blocks"]) => {
  const { used, heldUsed, level } = windowReading(
    shownRows(quotas, { blocks } as PanelLayout),
    now,
    shortestFirst,
  );
  return [used, heldUsed, level];
};

test("the number follows the shortest live window when the screen asks for it", () => {
  assert.deepEqual(reading(false), [90, null, "normal"]);
  assert.deepEqual(reading(true), [80, null, "normal"]);
});

test("the shortest window moves only the number: the level stays the tightest window's", () => {
  const spent = [
    row("claude-5h", 30, "normal", 18_000),
    row("claude-7d", 100, "critical", 604_800),
  ];
  const { used, level } = windowReading(spent, now, true);
  assert.deepEqual([used, level], [30, "critical"]);
});

test("ties on duration go to the highest used percent", () => {
  const tied = [
    row("claude-5h-a", 60, "normal", 18_000),
    row("claude-5h-b", 80, "normal", 18_000),
    row("claude-7d", 90, "normal", 604_800),
  ];
  const { used } = windowReading(tied, now, true);
  assert.equal(used, 80);
});

test("no duration data falls back to the tightest window", () => {
  const untimed = [
    row("claude-5h", 80, "normal", null),
    row("claude-7d", 90, "normal"),
  ];
  assert.deepEqual(
    (({ used, heldUsed, level }) => [used, heldUsed, level])(
      windowReading(untimed, now, true),
    ),
    [90, null, "normal"],
  );
});

test("stale and reset-passed windows never set the shortest number", () => {
  const stale = [
    row("claude-5h", 100, "stale", 18_000),
    row("claude-7d", 20, "normal", 604_800),
  ];
  assert.deepEqual(
    (({ used, heldUsed, level }) => [used, heldUsed, level])(
      windowReading(stale, now, true),
    ),
    [20, null, "normal"],
  );
  const passed = [
    row("claude-5h", 100, "critical", 18_000, hoursFromNow(-1)),
    row("claude-7d", 20, "normal", 604_800),
  ];
  assert.deepEqual(
    (({ used, heldUsed, level }) => [used, heldUsed, level])(
      windowReading(passed, now, true),
    ),
    [20, null, "normal"],
  );
});

test("the panel's hidden rows stay out of the shortest pick", () => {
  assert.deepEqual(
    reading(true, { claude: { hiddenRows: ["acct:claude-5h"] } }),
    [90, null, "normal"],
  );
  assert.deepEqual(reading(true, { claude: { hiddenAccounts: ["acct"] } }), [
    null,
    null,
    "stale",
  ]);
});

test("configuredVendors picks the shortest window per display, balance alone unchanged", () => {
  const snapshot = {
    language: "en",
    settings: { language: "en", quotaDisplay: "consumed" },
    quotas,
    billing: [
      {
        id: "ds:CNY",
        vendor: "deepseek",
        amount: 8.85,
        currency: "CNY",
        level: "normal",
      },
    ],
    sessions: [],
    approvals: [],
    providers: [],
  } as unknown as Snapshot;
  const marks = (shortestFirst: boolean) =>
    configuredVendors(snapshot, {
      ...defaultHUDConfig,
      mode: "simple",
      shortestWindowFirst: shortestFirst,
    });
  assert.equal(defaultHUDConfig.shortestWindowFirst, false);
  const claude = (shortestFirst: boolean) =>
    marks(shortestFirst).find((vendor) => vendor.key === "claude")!;
  assert.equal(meterLabel(claude(false)), "90%");
  assert.equal(meterLabel(claude(true)), "80%");
  const balance = marks(true).find((vendor) => vendor.key === "deepseek")!;
  assert.equal(balance.used, null);
  assert.equal(meterLabel(balance), "¥8.85");
  assert.equal(balance.level, "normal");
});

test("the switch counts only where Settings offers it: a Simple dock with numbers", () => {
  const on = (config: Partial<HUDConfig>) =>
    followsShortestWindow({
      ...defaultHUDConfig,
      shortestWindowFirst: true,
      ...config,
    });
  assert.equal(on({ mode: "simple" }), true);
  assert.equal(on({ mode: "simple", shortestWindowFirst: false }), false);
  assert.equal(on({ mode: "notch" }), false);
  assert.equal(
    on({ mode: "simple", simpleStyle: "rings", showsNumbers: false }),
    false,
  );
  assert.equal(
    on({ mode: "simple", simpleStyle: "numbers", showsNumbers: false }),
    true,
  );
  const snapshot = {
    language: "en",
    settings: { language: "en", quotaDisplay: "consumed" },
    quotas,
    billing: [],
    sessions: [],
    approvals: [],
    providers: [],
  } as unknown as Snapshot;
  const number = (config: Partial<HUDConfig>) =>
    meterLabel(
      configuredVendors(snapshot, {
        ...defaultHUDConfig,
        shortestWindowFirst: true,
        ...config,
      }).find((vendor) => vendor.key === "claude")!,
    );
  assert.equal(number({ mode: "simple" }), "80%");
  assert.equal(number({ mode: "notch" }), "90%");
  assert.equal(
    number({ mode: "simple", simpleStyle: "bars", showsNumbers: false }),
    "90%",
  );
});
