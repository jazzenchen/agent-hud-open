import { test } from "node:test";
import assert from "node:assert/strict";
import {
  closed,
  compareReleases,
  guideAtLaunch,
  latestNews,
  newsSince,
  nextStep,
  starAfterMs,
  tourOrder,
  type NewsEntry,
} from "../src/guide/plan.ts";

const news: NewsEntry[] = [
  { version: "0.6.2", items: [{ title: ["Two", "二"] }] },
  { version: "0.6.4", items: [{ title: ["Four", "四"] }] },
  { version: "0.7.0", items: [{ title: ["Seven", "七"] }] },
];
const record = (patch = {}) => ({
  since: 0,
  seen: "",
  step: "",
  star: "",
  ...patch,
});
const now = 1_800_000_000_000;

test("a beta and its release are one release", () => {
  assert.equal(compareReleases("0.6.2-beta.1", "0.6.2"), 0);
  assert.equal(compareReleases("0.6.10", "0.6.9"), 1);
  assert.equal(compareReleases("0.6", "0.6.0"), 0);
});

test("a new copy gets the tour, and a tour left open goes on", () => {
  assert.deepEqual(guideAtLaunch(record({ since: now }), "0.6.4", news, now), {
    kind: "tour",
    step: "hud",
  });
  assert.deepEqual(
    guideAtLaunch(record({ since: now, step: "plugins" }), "0.6.4", news, now),
    { kind: "tour", step: "plugins" },
  );
});

test("an update tells what came since the version last seen, up to the running one", () => {
  const titles = (seen: string, version: string) =>
    newsSince(seen, version, news).map((item) => item.title[0]);
  assert.deepEqual(titles("0.6.1", "0.6.4"), ["Two", "Four"]);
  assert.deepEqual(titles("0.6.2-beta.3", "0.6.2"), []);
  assert.deepEqual(titles("0.6.4", "0.6.4"), []);
  // A copy older than the guide hears only about the version it runs.
  assert.deepEqual(titles("", "0.6.4"), ["Four"]);
  assert.deepEqual(titles("", "0.6.3"), []);
  assert.equal(
    guideAtLaunch(record({ seen: "0.6.4" }), "0.6.4", news, now),
    null,
  );
});

test("the Star invitation waits a week in a new copy, and is asked once", () => {
  const fresh = record({ since: now - 1000, seen: "0.6.1" });
  const week = record({ since: now - starAfterMs, seen: "0.6.1" });
  const star = (r: ReturnType<typeof record>) => {
    const view = guideAtLaunch(r, "0.6.2", news, now);
    return view?.kind === "news" && view.star;
  };
  assert.equal(star(fresh), false);
  assert.equal(star(week), true);
  assert.equal(star(record({ seen: "0.6.1" })), true);
  assert.equal(star(record({ seen: "0.6.1", star: "done" })), false);
});

test("closing keeps the newest version seen, and the menu shows the newest news", () => {
  assert.equal(closed(record({ seen: "0.7.0" }), "0.6.4").seen, "0.7.0");
  assert.deepEqual(
    closed(record({ step: "display" }), "0.6.4"),
    record({ seen: "0.6.4" }),
  );
  const view = latestNews(record(), "0.6.5", news, now);
  assert.equal(view?.kind === "news" && view.version, "0.6.4");
  const older = latestNews(record(), "0.1.0", news, now);
  assert.equal(older?.kind === "news" && older.version, "0.7.0");
});

test("the tour goes from the HUD to each Settings row and ends with the Star", () => {
  const row = (id: string) => ({
    id,
    page: "agent-plugins",
    anchor: id,
    title: ["", ""] as [string, string],
    body: ["", ""] as [string, string],
  });
  const order = tourOrder([row("client-hooks"), row("remote")]);
  assert.deepEqual(order, ["hud", "mode", "client-hooks", "remote", "star"]);
  assert.equal(nextStep("mode", order), "client-hooks");
  assert.equal(nextStep("star", order), null);
  assert.equal(nextStep("gone", order), null);
});
