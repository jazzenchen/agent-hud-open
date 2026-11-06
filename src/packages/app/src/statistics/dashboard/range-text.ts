import type { Range } from "./model";

const DAYS: Record<string, number> = { "7d": 7, "30d": 30, "90d": 90 };

/** How the page names its range: the stretch it compares with ("the 7
 * days before", none where a comparison would mislead) and the range itself
 * ("in the last 7 days", "today", "on Nov 3"). */
export function rangeText(range: Range, from: number, zh: boolean) {
  const t = (en: string, cn: string) => (zh ? cn : en);
  const days = DAYS[range.id];
  const date = (language: string) =>
    new Date(from).toLocaleDateString(language, {
      month: "short",
      day: "numeric",
    });
  return {
    compared:
      range.id === "day"
        ? t("the day before", "前一天")
        : days
          ? t(`the ${days} days before`, `前 ${days} 天`)
          : null,
    words:
      range.id === "day"
        ? t(`on ${date("en-US")}`, `在 ${date("zh-CN")}`)
        : range.id === "today"
          ? t("today", "今天")
          : range.id === "1y"
            ? t("in the last year", "近一年")
            : t(`in the last ${days} days`, `近 ${days} 天`),
  };
}
