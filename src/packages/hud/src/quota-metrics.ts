import type { Quota, QuotaMeasure } from "@agent-hud/protocol";
import {
  budgetText,
  moneyShort,
  quotaPercentLabel,
  quotaDisplayLabel,
  monthDayTime,
  outlookBasisText,
  outlookText,
  relativeWeekdayTime,
  resetLabel,
  samplingHint,
  samplingLabel,
  tokensShort,
  type FormatSettings,
} from "@agent-hud/ui/format";
import { exhaustionHint, missing, say } from "./quota-metric-copy.ts";

export interface QuotaMetricReading {
  value: string;
  detail: string;
  hint: string;
  available: boolean;
  selectable: boolean;
  runsOut: boolean;
  projectedPercent: number | null;
  /** Where Core's budget, a steady pace through the cycle, has the used
   * share now, for a mark on the bar; null without one. */
  budgetPercent: number | null;
}

/** Open's AgentRow.resetLabel: "—" without a reset, "Pending update" once
 * it has passed with no reading to confirm it, "<1m" inside a minute. */
function rowResetLabel(row: Quota, now: number, settings: FormatSettings) {
  const resetAt = row.resetAt ? Date.parse(row.resetAt) : NaN;
  if (Number.isFinite(resetAt)) {
    if (resetAt <= now)
      return say(settings.language, "等待更新", "Pending update");
    if (resetAt - now < 60_000) return "<1m";
  }
  return resetLabel(
    Number.isFinite(resetAt) ? resetAt : null,
    now,
    settings.calendar,
    settings.language,
  );
}

/** What a quota row's measure reads: missing quota is "--", other missing
 * measures and details are "—". A reading that shows no level, stale here,
 * has no burn rate, outlook, reply rate, consumption or cost. Formatting only: the UI never
 * assigns session totals or rates to a quota window. */
export function quotaMetric(
  row: Quota,
  measure: QuotaMeasure,
  now: number,
  settings: FormatSettings,
): QuotaMetricReading {
  const { language, calendar } = settings;
  const metrics = row.metrics;
  const result: QuotaMetricReading = {
    value: "—",
    detail: "—",
    available: false,
    selectable: measure === "quota",
    runsOut: false,
    projectedPercent: null,
    budgetPercent: null,
    hint: metrics?.unavailable?.[measure] ?? missing(measure, language),
  };
  const stale = say(
    language,
    "额度读数已过期，请刷新这个账户。",
    "Last quota reading is stale; refresh this account.",
  );
  const budget = metrics?.budget ?? null;
  const budgetLine = budget ? budgetText(budget, language) : null;
  if (measure === "quota") {
    result.value = "--";
    result.detail = rowResetLabel(row, now, settings);
    if (row.usedPercent != null) {
      result.value = quotaPercentLabel(row.usedPercent, settings.quotaDisplay);
      result.available = true;
      result.hint =
        row.status === "stale"
          ? stale
          : `${row.name}: ${result.value} · ${quotaDisplayLabel(settings.quotaDisplay, language)} · ${result.detail}${budgetLine ? ` · ${budgetLine}` : ""}`;
    }
    return result;
  }
  if (row.status === "stale" || row.usedPercent == null) {
    result.hint = row.status === "stale" ? stale : missing("quota", language);
    return result;
  }
  const outlook = metrics?.outlook;
  result.runsOut =
    outlook?.kind === "exhausted" ||
    (outlook?.kind === "exhausts" && outlook.beforeReset === true);
  if (measure === "burn") {
    const exhaustsAt = outlook?.exhaustsAt
      ? Date.parse(outlook.exhaustsAt)
      : NaN;
    const projected = outlook?.projectedUsedPercent ?? null;
    if (metrics?.burnRatePctPerHour != null) {
      result.value = `${metrics.burnRatePctPerHour.toFixed(1)}%/h`;
      result.available = result.selectable = true;
    } else if (outlook?.basis === "window" && outlook.ratePctPerHour != null) {
      // The cycle's average pace so far, until recent readings give one.
      result.value = `~${outlook.ratePctPerHour.toFixed(1)}%/h`;
      result.available = result.selectable = true;
    }
    result.budgetPercent = budget ? budget.expectedUsedPercent : null;
    const sampling =
      outlook?.kind === "insufficient-data" ? metrics?.sampling : null;
    if (sampling) result.selectable = true;
    const words = outlook
      ? outlookText(
          {
            kind: outlook.kind,
            seconds: Number.isFinite(exhaustsAt)
              ? Math.max(0, (exhaustsAt - now) / 1000)
              : undefined,
            beforeReset: outlook.beforeReset,
          },
          projected,
          language,
        )
      : null;
    // The row gives the time the rest runs out at; the hint, how long it lasts.
    result.detail =
      outlook?.kind === "exhausts" &&
      outlook.beforeReset === true &&
      Number.isFinite(exhaustsAt)
        ? exhaustsAt - now < 7 * 86_400_000
          ? relativeWeekdayTime(exhaustsAt, now, calendar, language)
          : monthDayTime(exhaustsAt, calendar, language)
        : sampling
          ? samplingLabel(sampling.afterReset, language)
          : (words ?? "—");
    result.projectedPercent = result.runsOut ? 100 : projected;
    const basis =
      outlook?.basis === "window"
        ? say(
            language,
            `${outlookBasisText("window", language)}，读数够了改按目前节奏`,
            `${outlookBasisText("window", language)} until readings give the recent pace`,
          )
        : outlook?.basis === "recent"
          ? outlookBasisText("recent", language)
          : null;
    const lines = sampling
      ? [
          samplingHint(
            sampling.elapsed,
            sampling.required,
            sampling.afterReset,
            language,
          ),
        ]
      : result.available
        ? [
            `${result.value} · ${words ?? say(language, "这个周期的读数", "from this quota cycle's readings")}`,
            basis,
          ]
        : [];
    if (lines.length) {
      if (budgetLine) lines.push(budgetLine);
      result.hint = lines.filter(Boolean).join("\n");
    }
  } else if (measure === "tokens" && metrics?.replyRate) {
    const rate = metrics.replyRate;
    // A rate timed by the gaps between calls holds tool runs and the
    // person's time, so it is a floor.
    result.value = `${rate.lowerBound ? "≥" : ""}${rate.tokensPerSecond < 10 ? rate.tokensPerSecond.toFixed(1) : Math.trunc(rate.tokensPerSecond)}/s`;
    result.detail = say(
      language,
      `${rate.calls} 次回复`,
      `${rate.calls} replies`,
    );
    result.available = result.selectable = true;
    result.hint = exhaustionHint(outlook, metrics.sampling, now, settings);
  } else if (
    measure === "consumed" &&
    metrics?.consumed &&
    metrics.consumed.used >= 0
  ) {
    const used = metrics.consumed;
    result.value = tokensShort(used.used);
    // With a capacity the line reads "used / cap"; without one, the pace.
    result.detail =
      used.capacity != null && used.capacity >= 0
        ? `/ ${tokensShort(used.capacity)}`
        : metrics.tokensPerHour != null
          ? `${tokensShort(Math.round(metrics.tokensPerHour))}/h`
          : "—";
    result.available = result.selectable = true;
    const estimate = metrics.capacityEstimate;
    const total = estimate ? tokensShort(estimate.tokens) : "—";
    result.hint = [
      say(
        language,
        `预计总 Token：${total}`,
        `Estimated total tokens: ${total}`,
      ),
      exhaustionHint(outlook, metrics.sampling, now, settings),
    ].join("\n");
  } else if (measure === "cost" && metrics?.cost) {
    const cost = metrics.cost;
    const currencies = Object.keys(cost.amounts).sort();
    if (currencies.length) {
      const first = currencies[0];
      result.value = moneyShort(cost.amounts[first], first, language);
      // A second currency, which is rare, is the detail; else the pace.
      result.detail =
        currencies.length > 1
          ? `+ ${currencies
              .slice(1)
              .map((currency) =>
                moneyShort(cost.amounts[currency], currency, language),
              )
              .join(" + ")}`
          : `${moneyShort(cost.amounts[first] / cost.hours, first, language)}/h`;
      result.available = result.selectable = true;
      const amounts =
        Object.entries(metrics.capacityEstimate?.amounts ?? {})
          .sort(([left], [right]) => left.localeCompare(right))
          .map(([currency, amount]) => moneyShort(amount, currency, language))
          .join(" · ") || "—";
      result.hint = [
        say(
          language,
          `预计总额度：${amounts}`,
          `Estimated total capacity: ${amounts}`,
        ),
        exhaustionHint(outlook, metrics.sampling, now, settings),
      ].join("\n");
    }
  }
  return result;
}

export function quotaMetricAvailability(
  rows: readonly Quota[],
  measure: QuotaMeasure,
  now: number,
  settings: FormatSettings,
) {
  const readings = rows.map((row) => quotaMetric(row, measure, now, settings));
  const selectable = readings.some((reading) => reading.selectable);
  return {
    selectable,
    hint: selectable
      ? readings.find((reading) => reading.selectable)!.hint
      : [...new Set(readings.map((reading) => reading.hint))].join("\n") ||
        missing(measure, settings.language),
  };
}
