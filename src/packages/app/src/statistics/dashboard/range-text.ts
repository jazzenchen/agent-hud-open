import { locale, translate, type Language } from "@agent-hud/ui/format";
import type { Range } from "./model";

const DAYS: Record<string, number> = { "7d": 7, "30d": 30, "90d": 90 };

/** How the page names its range: the stretch it compares with ("the 7
 * days before", none where a comparison would mislead) and the range itself
 * ("in the last 7 days", "today", "on Nov 3"). */
export function rangeText(range: Range, from: number, language: Language) {
  const days = DAYS[range.id];
  const date = () =>
    new Date(from).toLocaleDateString(locale(language), {
      month: "short",
      day: "numeric",
    });
  return {
    compared:
      range.id === "day"
        ? translate(language, "the day before")
        : days
          ? translate(language, "the {{days}} days before", { days })
          : null,
    words:
      range.id === "day"
        ? translate(language, "on {{date}}", { date: date() })
        : range.id === "today"
          ? translate(language, "today")
          : range.id === "1y"
            ? translate(language, "in the last year")
            : translate(language, "in the last {{days}} days", { days }),
  };
}
