import type { Quota, QuotaMeasure } from "@agent-hud/protocol";
import {
  moneyShort,
  quotaPercentLabel,
  quotaDisplayLabel,
  monthDayTime,
  numberFormat,
  outlookText,
  relativeWeekdayTime,
  resetLabel,
  samplingHint,
  samplingLabel,
  tokensShort,
  type FormatSettings,
  type Language,
} from "@agent-hud/ui/format";

export interface QuotaMetricReading {
  value: string;
  detail: string;
  hint: string;
  available: boolean;
  selectable: boolean;
  runsOut: boolean;
  projectedPercent: number | null;
}
const finite = (value: number | null | undefined): value is number =>
  value != null && Number.isFinite(value);
const nonnegative = (value: number | null | undefined): value is number =>
  finite(value) && value >= 0;

/** TokenFormat.short; kept for callers that format a count of their own. */
export const formatTokens = tokensShort;

const englishUTC: FormatSettings = {
  language: "en",
  calendar: { timeZone: "UTC", firstWeekday: 1 },
};

const say = (language: Language, zh: string, en: string) =>
  language === "zh" ? zh : en;

/** Why a measure has no value, where Core gave no reason of its own. */
function missing(measure: QuotaMeasure, language: Language) {
  switch (measure) {
    case "quota":
      return say(
        language,
        "这个窗口暂无当前额度读数。",
        "No current quota reading is available for this window.",
      );
    case "burn":
      return say(
        language,
        "消耗速度需要本周期内跨越最短采样时长的读数。",
        "Burn rate needs readings spanning the minimum sampling interval in this quota cycle.",
      );
    case "tokens":
      return say(
        language,
        "输出速度需要这个窗口模型的计时调用：同一日志中至少一对相隔不超过 120 秒的记录。",
        "Reply rate needs timed calls from this window's models: at least one same-log pair no more than 120 seconds apart.",
      );
    case "consumed":
      return say(
        language,
        "已消耗需要来源自己的计数，或已知周期内对应模型的用量。",
        "Consumed needs the source's tally, or a known quota cycle with matching model usage.",
      );
    case "cost":
      return say(
        language,
        "这项估算需要已知的额度周期、对应模型的用量和这些模型的标价。",
        "This estimate needs a known quota cycle, matching model usage and list prices for its models.",
      );
  }
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
  settings: FormatSettings = englishUTC,
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
    hint: metrics?.unavailable?.[measure] ?? missing(measure, language),
  };
  const stale = say(
    language,
    "额度读数已过期，请刷新这个账户。",
    "Last quota reading is stale; refresh this account.",
  );
  if (measure === "quota") {
    result.value = "--";
    result.detail = rowResetLabel(row, now, settings);
    if (finite(row.usedPercent)) {
      result.value = quotaPercentLabel(row.usedPercent, settings.quotaDisplay);
      result.available = true;
      result.hint =
        row.status === "stale"
          ? stale
          : `${row.name}: ${result.value} · ${quotaDisplayLabel(settings.quotaDisplay, language)} · ${result.detail}`;
    }
    return result;
  }
  if (row.status === "stale" || !finite(row.usedPercent)) {
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
    const projected = finite(outlook?.projectedUsedPercent)
      ? outlook.projectedUsedPercent
      : null;
    if (nonnegative(metrics?.burnRatePctPerHour)) {
      result.value = `${metrics.burnRatePctPerHour.toFixed(1)}%/h`;
      result.available = result.selectable = true;
    }
    const sample = metrics?.sampling;
    const sampling =
      outlook?.kind === "insufficient-data" &&
      sample &&
      nonnegative(sample.elapsed) &&
      nonnegative(sample.required) &&
      sample.required > 0
        ? sample
        : null;
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
    result.hint = sampling
      ? samplingHint(
          sampling.elapsed,
          sampling.required,
          sampling.afterReset,
          language,
        )
      : result.available
        ? `${result.value} · ${words ?? say(language, "这个周期的读数", "from this quota cycle's readings")}`
        : result.hint;
  } else if (
    measure === "tokens" &&
    metrics?.replyRate &&
    nonnegative(metrics.replyRate.tokensPerSecond) &&
    metrics.replyRate.calls > 0
  ) {
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
    const zhTokens = numberFormat("zh-CN").format(rate.tokensOut);
    const enTokens = numberFormat("en-US").format(rate.tokensOut);
    result.hint = rate.lowerBound
      ? say(
          language,
          `这个窗口的模型在 ${rate.calls} 次回复中输出 ${zhTokens} 个 token。客户端日志没有记下每次调用的用时，只能从上一次调用算起（120 秒以内），其中含工具运行和你的时间，实际速度不低于这个数。有最近一小时的记录时取最近一小时，否则取最近七天。`,
          `${enTokens} output tokens over ${rate.calls} replies from this window's models. The client's log does not time its calls, so each is timed from the call before it (within 120 seconds), tool runs and your time included; the real rate is at least this. Latest hour when available, otherwise the last seven days.`,
        )
      : say(
          language,
          `这个窗口的模型在 ${rate.calls} 次回复中输出 ${zhTokens} 个 token，每次按客户端日志记下的调用用时计算，不含工具运行和你的时间。有最近一小时的记录时取最近一小时，否则取最近七天。`,
          `${enTokens} output tokens over ${rate.calls} replies from this window's models, each timed by its own call as the client's log records it, without tool runs or your time. Latest hour when available, otherwise the last seven days.`,
        );
  } else if (
    measure === "consumed" &&
    metrics?.consumed &&
    nonnegative(metrics.consumed.used)
  ) {
    const used = metrics.consumed;
    result.value = formatTokens(used.used);
    // With a capacity the line reads "used / cap"; without one, the pace.
    result.detail = nonnegative(used.capacity)
      ? `/ ${formatTokens(used.capacity)}`
      : nonnegative(metrics.tokensPerHour)
        ? `${formatTokens(Math.round(metrics.tokensPerHour))}/h`
        : "—";
    result.available = result.selectable = true;
    const since = metrics.observedFrom ? Date.parse(metrics.observedFrom) : NaN;
    result.hint = Number.isFinite(since)
      ? say(
          language,
          `这个额度窗口已消耗${used.unit ? `（${used.unit}）` : ""}，自 ${monthDayTime(since, calendar, language)} 起记录。`,
          `Consumed in this quota window${used.unit ? ` (${used.unit})` : ""}, observed since ${monthDayTime(since, calendar, language)}.`,
        )
      : say(
          language,
          "只计入已知周期内观察到的用量。",
          "Only observed usage in its known cycle is counted.",
        );
  } else if (
    measure === "cost" &&
    metrics?.cost &&
    finite(metrics.cost.hours) &&
    metrics.cost.hours > 0
  ) {
    const cost = metrics.cost;
    const currencies = Object.keys(cost.amounts)
      .filter(
        (currency) =>
          /^[A-Z]{3}$/.test(currency) && nonnegative(cost.amounts[currency]),
      )
      .sort();
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
      result.hint = say(
        language,
        "按标价估算这个窗口本周期已观察到的模型用量。订阅费不按 token 计费；不同币种分开计算。",
        "List-price estimate for this window's observed current-cycle model usage. Subscription fees are not billed by token. Currencies remain separate.",
      );
    }
  }
  return result;
}

export function quotaMetricAvailability(
  rows: readonly Quota[],
  measure: QuotaMeasure,
  now: number,
  settings: FormatSettings = englishUTC,
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
