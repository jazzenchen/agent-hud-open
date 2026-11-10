import { test } from "node:test";
import assert from "node:assert/strict";
import {
  quotaAlertCopy,
  resetCreditCopy,
} from "../src/alerts/quota-alert-copy.ts";

const en = {
  language: "en" as const,
  calendar: { timeZone: "UTC", firstWeekday: 1 },
};
const zh = {
  language: "zh" as const,
  calendar: { timeZone: "UTC", firstWeekday: 2 },
};
const event = (extra: object) =>
  ({
    id: "e",
    kind: "quota-exhaustion",
    at: "2026-10-21T12:00:00Z",
    source: "codex",
    title: "Weekly",
    ...extra,
  }) as never;

test("quota cards say what Open says, measured from the reading", () => {
  const empty = quotaAlertCopy(
    event({
      usedPercent: 100,
      crossing: "exhausted",
      resetAt: "2026-10-23T09:00:00Z",
    }),
    en,
  );
  assert.deepEqual(
    [empty.title, empty.compactTitle, empty.used, empty.exhaustion],
    ["This window is empty. Waiting for reset.", "Empty", "100%", "—"],
  );
  assert.notEqual(empty.reset, "45h"); // more than a day away: a weekday and time, not a long countdown
  const forecast = quotaAlertCopy(
    event({
      usedPercent: 92,
      timeToExhaustSeconds: 3_000,
      resetAt: "2026-10-21T15:00:00Z",
    }),
    zh,
  );
  assert.deepEqual(
    [forecast.compactTitle, forecast.exhaustion, forecast.reset],
    ["50m 后耗尽", "50m", "3h00m"],
  );
  assert.equal(quotaAlertCopy(event({}), en).used, "N/A");
  const reset = quotaAlertCopy(
    event({
      kind: "quota-reset",
      usedPercent: 0,
      otherExhaustedWindows: [{ name: "Spark" }],
    }),
    en,
  );
  assert.deepEqual(
    [reset.title, reset.otherLimits],
    [
      "This window has reset; other limits still apply.",
      "Still exhausted: Spark",
    ],
  );
});

test("a credit card names the first expiry as a date", () => {
  const copy = resetCreditCopy(
    2,
    ["2026-11-09T08:00:00Z", "2026-11-02T08:00:00Z", null],
    "en",
  );
  assert.deepEqual(
    [copy.compactTitle, copy.title, copy.expiry],
    ["+2 resets", "2 usage resets were added to this account.", "Nov 2"],
  );
});

test("remaining display keeps alert thresholds and forecasts in consumed semantics", () => {
  const reading = event({ usedPercent: 92, timeToExhaustSeconds: 3000 });
  const consumed = quotaAlertCopy(reading, { ...en, quotaDisplay: "consumed" });
  const remaining = quotaAlertCopy(reading, {
    ...en,
    quotaDisplay: "remaining",
  });
  assert.equal(remaining.display, "8%");
  assert.equal(remaining.displayPct, 8);
  assert.equal(remaining.displayLabel, "Quota remaining");
  assert.equal(consumed.display, "92%");
  assert.equal(remaining.title, consumed.title);
  assert.equal(remaining.compactTitle, consumed.compactTitle);
  assert.equal(remaining.exhaustion, consumed.exhaustion);
  assert.equal(
    quotaAlertCopy(event({ usedPercent: 100 }), {
      ...en,
      quotaDisplay: "remaining",
    }).display,
    "0%",
  );
  assert.equal(
    quotaAlertCopy(event({ usedPercent: 0, kind: "quota-reset" }), {
      ...en,
      quotaDisplay: "remaining",
    }).display,
    "100%",
  );
  assert.equal(
    quotaAlertCopy(event({}), { ...en, quotaDisplay: "remaining" }).display,
    "N/A",
  );
});
