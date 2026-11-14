// What a quota row's measure says when it has no value, where Core gave no
// reason of its own; kept apart from the readings, as quota-alert-copy is.
import type { QuotaMeasure } from "@agent-hud/protocol";
import { translate, type Language } from "@agent-hud/ui/format";

/** Why a measure has no value, where Core gave no reason of its own. */
export function missing(measure: QuotaMeasure, language: Language) {
  switch (measure) {
    case "quota":
      return translate(
        language,
        "No current quota reading is available for this window.",
      );
    case "burn":
      return translate(
        language,
        "Burn rate needs readings spanning the minimum sampling interval in this quota cycle.",
      );
    case "tokens":
      return translate(
        language,
        "Reply rate needs at least 3 timed replies from this window's models, 5 seconds in all: each timed by its log, or from the same log's call before it within 120 seconds.",
      );
    case "consumed":
      return translate(
        language,
        "Consumed needs the source's tally, or a known quota cycle with matching model usage.",
      );
    case "cost":
      return translate(
        language,
        "This estimate needs a known quota cycle, matching model usage and list prices for its models.",
      );
  }
}
