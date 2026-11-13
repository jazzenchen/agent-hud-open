// What the guide shows, and how each answer changes its record
// (`AppSettings.guide`). A new copy gets the tour: its first steps in a small
// window beside the HUD, the rest in Settings, on the rows they are about,
// ending with the Star. A copy that moved to a newer version gets that
// version's news beside the HUD, once.
import type { GuideRecord, Snapshot } from "@agent-hud/protocol";

/** The tour's steps beside the HUD, in order. */
export const hudSteps = ["hud", "mode"] as const;
/** The tour's last step, in Settings, after every row: the Star. */
export const starStep = "star";

/** A tour step in Settings: a row on a page, marked `data-guide={anchor}`;
 * several names, space-separated, ring the first that is on the page. */
export interface SettingsStep {
  id: string;
  page: string;
  anchor: string;
  title: [en: string, zh: string];
  body: [en: string, zh: string];
  /** Already set up: the step is ticked. */
  done?: (snapshot: Snapshot) => boolean;
}

/** A place in Settings: a page, and the element on it marked
 * `data-guide={anchor}`; without a page, the element on any page. */
export interface GuideTarget {
  page?: string;
  anchor?: string;
}

export interface NewsItem {
  title: [en: string, zh: string];
  body?: [en: string, zh: string];
  target?: GuideTarget;
}

/** What a release adds, in its own words, a few items at most. */
export interface NewsEntry {
  version: string;
  items: NewsItem[];
}

export type GuideView =
  | { kind: "tour"; step: string }
  | { kind: "news"; version: string; items: NewsItem[]; star: boolean };

/** How long a new copy is used before the news asks for a Star. */
export const starAfterMs = 7 * 24 * 60 * 60 * 1000;

/** Every step of the tour, in order. */
export function tourOrder(settings: readonly SettingsStep[]): string[] {
  return [...hudSteps, ...settings.map((step) => step.id), starStep];
}

export function isHUDStep(step: string): boolean {
  return (hudSteps as readonly string[]).includes(step);
}

/** A version's release numbers, without a pre-release part: 0.6.2-beta.1
 * and 0.6.2 are one release here, so a beta's news is not shown again. */
function release(version: string): number[] {
  return version
    .split("-")[0]
    .split(".")
    .map((part) => Number.parseInt(part, 10) || 0);
}

export function compareReleases(a: string, b: string): number {
  const left = release(a);
  const right = release(b);
  for (let index = 0; index < Math.max(left.length, right.length); index++) {
    const difference = (left[index] ?? 0) - (right[index] ?? 0);
    if (difference) return Math.sign(difference);
  }
  return 0;
}

/** The news since `seen`, up to the running version, oldest first. A copy
 * that has seen none hears only about the version it runs. */
export function newsSince(
  seen: string,
  version: string,
  entries: readonly NewsEntry[],
): NewsItem[] {
  return [...entries]
    .filter(
      (entry) =>
        compareReleases(entry.version, version) <= 0 &&
        (seen
          ? compareReleases(entry.version, seen) > 0
          : compareReleases(entry.version, version) === 0),
    )
    .sort((a, b) => compareReleases(a.version, b.version))
    .flatMap((entry) => entry.items);
}

export function starDue(record: GuideRecord, now: number): boolean {
  return (
    record.star !== "done" &&
    (record.since === 0 || now - record.since >= starAfterMs)
  );
}

/** What opens at launch: a tour left open goes on where it stopped, a new
 * copy starts one, and otherwise the news since the last version seen. */
export function guideAtLaunch(
  record: GuideRecord | undefined,
  version: string,
  entries: readonly NewsEntry[],
  now: number,
): GuideView | null {
  if (!record) return null;
  if (record.step) return { kind: "tour", step: record.step };
  if (!record.seen && record.since > 0) return { kind: "tour", step: "hud" };
  const items = newsSince(record.seen, version, entries);
  return items.length
    ? { kind: "news", version, items, star: starDue(record, now) }
    : null;
}

/** The newest news, for the menu's What's New: the running version's, or
 * the newest written when a build is older than every entry. */
export function latestNews(
  record: GuideRecord,
  version: string,
  entries: readonly NewsEntry[],
  now: number,
): GuideView | null {
  const sorted = [...entries].sort((a, b) =>
    compareReleases(b.version, a.version),
  );
  const entry =
    sorted.find((item) => compareReleases(item.version, version) <= 0) ??
    sorted[0];
  return entry
    ? {
        kind: "news",
        version: entry.version,
        items: entry.items,
        star: starDue(record, now),
      }
    : null;
}

/** The record once the guide on screen is finished or closed. */
export function closed(record: GuideRecord, version: string): GuideRecord {
  const seen =
    record.seen && compareReleases(record.seen, version) > 0
      ? record.seen
      : version;
  return { ...record, seen, step: "" };
}

export function atStep(record: GuideRecord, step: string): GuideRecord {
  return { ...record, step };
}

export function starAnswered(record: GuideRecord): GuideRecord {
  return { ...record, star: "done" };
}

/** The step after `step` in `order`, or null after the last. */
export function nextStep(
  step: string,
  order: readonly string[],
): string | null {
  const index = order.indexOf(step);
  return index < 0 ? null : (order[index + 1] ?? null);
}

/** What a product adds to the guide: its releases' news, and tour steps on
 * its own Settings pages, after Agent HUD's. */
export interface ProductGuide {
  news?: NewsEntry[];
  steps?: SettingsStep[];
}

/** The step before `step` in `order`, or null at the first. */
export function previousStep(
  step: string,
  order: readonly string[],
): string | null {
  const index = order.indexOf(step);
  return index > 0 ? order[index - 1] : null;
}

/** Where the project lives, for the Star. */
export const projectPage = "https://github.com/jazzenchen/agent-hud-open";
