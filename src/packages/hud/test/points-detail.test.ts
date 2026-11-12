import { test } from "node:test";
import assert from "node:assert/strict";
import {
  dailyPaceLabel,
  grantExpiryLabel,
  grantPointsLabel,
  leadingPointsPercent,
  pointsAmount,
  pointsMeasureAvailability,
  pointsSpent,
  remainingPercentLabel,
  usedPointsPercent,
} from "../src/panel/points-detail.ts";

test("TeleAgent points read as a total, a remaining share and an expiring grant", () => {
  assert.equal(pointsAmount(19700, "zh"), "19,700");
  assert.equal(pointsAmount(19575.06, "zh"), "19,575.06");
  assert.equal(remainingPercentLabel(99.37, "zh"), "99.37%");
  assert.equal(grantPointsLabel(12875, "zh"), "12,875 积分");
  assert.equal(grantExpiryLabel(18, "zh"), "18 天后到期");
  assert.equal(grantExpiryLabel(0, "zh"), "今天到期");
  assert.equal(grantPointsLabel(12875, "en"), "12,875 points");
  assert.equal(grantExpiryLabel(18, "en"), "Expires in 18 days");
  assert.equal(
    leadingPointsPercent([{ percent: null }, { percent: 99.37 }]),
    99.37,
  );
  assert.equal(usedPointsPercent(99.37), 0.63);
  const measures = pointsMeasureAvailability("zh");
  assert.equal(measures.quota.selectable, true);
  assert.equal(measures.consumed.selectable, true);
  assert.equal(measures.burn.selectable, false);
  assert.equal(measures.tokens.selectable, false);
  assert.equal(measures.cost.selectable, false);
  assert.equal(pointsSpent(19700, 19575.06), 124.94);
  assert.equal(dailyPaceLabel(10, "zh"), "10/天");
});
