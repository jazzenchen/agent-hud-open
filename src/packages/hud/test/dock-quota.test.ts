import assert from "node:assert/strict";
import { test } from "node:test";
import type { Quota } from "@agent-hud/protocol";
import {
  dockQuotaWindows,
  takeQuotaAlert,
} from "../src/usage-meter/dock-quota.ts";
import { dockNumberColors } from "../src/usage-meter/dock-number-style.ts";

const row = (
  id: string,
  hours: number | null,
  usedPercent: number,
  status: Quota["status"] = "normal",
): Quota => ({
  id,
  vendor: "codex",
  name: id,
  account: null,
  resetAt: null,
  windowDurationSeconds: hours === null ? null : hours * 3600,
  usedPercent,
  status,
});

test("the shortest live quota wins over the most used one", () => {
  const rows = [row("weekly", 168, 38), row("5h", 5, 7), row("1h", 1, 4)];
  assert.equal(dockQuotaWindows(rows.slice(0, 2), 10).primary?.id, "5h");
  assert.equal(dockQuotaWindows(rows, 10).primary?.id, "1h");
  assert.equal(dockQuotaWindows(rows, 10).warning, undefined);
});

test("the threshold is strict and Never disables warnings", () => {
  for (const threshold of [5, 10, 15, 20, 30]) {
    const short = row("5h", 5, 50);
    const weekly = row("weekly", 168, 100 - threshold);
    assert.equal(
      dockQuotaWindows([short, weekly], threshold).warning,
      undefined,
    );
    weekly.usedPercent = 100 - threshold + 0.1;
    assert.equal(
      dockQuotaWindows([short, weekly], threshold).warning?.id,
      "weekly",
    );
    assert.equal(dockQuotaWindows([short, weekly], 0).warning, undefined);
  }
});

test("unknown, stale, and same-length rows cannot trigger a long warning", () => {
  const rows = [
    row("unknown", null, 100),
    row("stale", 1, 100, "stale"),
    row("5h", 5, 50),
    row("another 5h", 5, 98),
    row("weekly", 168, 91),
    row("monthly", 720, 93),
  ];
  const result = dockQuotaWindows(rows, undefined);
  assert.equal(result.primary?.id, "another 5h");
  assert.equal(result.warning?.id, "monthly");
  assert.deepEqual(
    result.warningWindows.map((q) => q.id),
    ["weekly", "monthly"],
  );
});

test("the first-crossing alert survives a remount and rearms on the next reset", () => {
  const saved = new Map<string, string>();
  const storage = () => ({
    getItem: (key: string) => saved.get(key) ?? null,
    setItem: (key: string, value: string) => void saved.set(key, value),
  });
  const first = { id: "weekly", resetAt: "2026-11-17T07:55:00Z" };
  assert.equal(takeQuotaAlert(storage(), first), true);
  assert.equal(takeQuotaAlert(storage(), first), false);
  assert.equal(
    takeQuotaAlert(storage(), { ...first, resetAt: "2026-11-24T07:55:00Z" }),
    true,
  );
});

test("quota labels color only the text, with adjustable opacity in grayscale", () => {
  assert.deepEqual(dockNumberColors("grayscale", 0.75, true, "green"), {
    color: "#262626",
    opacity: 0.25,
  });
  assert.deepEqual(dockNumberColors("tinted", 0.75, true, "green"), {
    color: "green",
  });
});
