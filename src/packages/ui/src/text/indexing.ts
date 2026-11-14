// What a window says while the usage ledger is read through: built for the
// first time ("first"), or again after a data-version change ("reindex").
// Logs are read newest first, so the reading moves back day by day from
// today toward the day the oldest log was last written; the share counts
// those days. Nothing shows once the run ends.
import { dateFormat } from "../format/intl.ts";
import {
  isChinese,
  locale,
  translator,
  type Language,
} from "../format/language.ts";

/** The snapshot's `indexing`, as far as this text needs it. */
export interface IndexingFacts {
  kind?: string | null;
  bytesDone?: number;
  bytesTotal?: number;
  oldestLogAt?: string | null;
  readBackTo?: string | null;
}

export interface IndexingText {
  /** 0…1, short of 1 until the run ends. */
  fraction: number;
  /** The run and how far back it has read, e.g. "Reading your history · back to Sep 12". */
  title: string;
  /** Days read of all, e.g. "54 of 184 days"; empty when not known. */
  days: string;
  /** How far back the logs go, and what is incomplete meanwhile. */
  detail: string;
}

/** A calendar day on this computer's clock, counted in days. */
function dayNumber(at: Date): number {
  return Math.round(
    Date.UTC(at.getFullYear(), at.getMonth(), at.getDate()) / 86_400_000,
  );
}

function parsed(text: string | null | undefined): Date | null {
  if (!text) return null;
  const at = new Date(text);
  return Number.isNaN(at.getTime()) ? null : at;
}

/**
 * "Sep 12" or "9月12日", with the year when it is not this one. */
function dayText(at: Date, now: Date, language: Language): string {
  const year =
    at.getFullYear() === now.getFullYear() ? {} : { year: "numeric" as const };
  return dateFormat(locale(language), {
    month: isChinese(language) ? "long" : "short",
    day: "numeric",
    ...year,
  }).format(at);
}

export function indexingText(
  indexing: IndexingFacts | null | undefined,
  language: Language,
  now: Date = new Date(),
): IndexingText | null {
  const kind = indexing?.kind;
  if (kind !== "first" && kind !== "reindex") return null;
  const t = translator(language);
  const oldest = parsed(indexing?.oldestLogAt);
  // With none left the reading has reached the oldest log.
  const reached = parsed(indexing?.readBackTo) ?? oldest;
  // A run is not done until the core says so: 100% waits for that.
  const share = (done: number, all: number) =>
    all > 0 ? Math.min(Math.max(done, 0) / all, 0.99) : 0;
  if (!oldest || !reached) {
    const total = Math.max(0, indexing?.bytesTotal ?? 0);
    return {
      fraction: share(Math.min(indexing?.bytesDone ?? 0, total), total),
      title:
        kind === "first"
          ? t("Reading your history")
          : t("Reading your history again"),
      days: "",
      detail: t("Tokens and costs are incomplete until it finishes"),
    };
  }
  const today = dayNumber(now);
  const all = Math.max(1, today - dayNumber(oldest) + 1);
  // The days after the one being read hold only logs already read.
  const done = Math.min(Math.max(0, today - dayNumber(reached)), all);
  const day = dayText(reached, now, language);
  return {
    fraction: share(done, all),
    title:
      kind === "first"
        ? t("Reading your history · back to {{day}}", { day })
        : t("Reading your history again · back to {{day}}", { day }),
    days: t("{{done}} of {{count}} days", { done, count: all }),
    detail: t(
      "Your logs go back to {{day}} · tokens and costs are incomplete until it finishes",
      { day: dayText(oldest, now, language) },
    ),
  };
}
