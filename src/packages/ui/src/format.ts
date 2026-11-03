// Display formatting for tokens, money and time. Follows agent-hud-native-open's
// TokenFormat, MoneyFormat and Countdown; the shared fixtures in
// src/fixtures/parity check every function here, including the improvements
// recorded in src/fixtures/parity/decisions.json.

export type Language = "en" | "zh";
export type QuotaDisplay = "consumed" | "remaining";

/** Presentation only: unknown readings stay unknown and source usage stays unchanged. */
export function quotaPercent(
  used: number | null | undefined,
  display: QuotaDisplay = "consumed",
): number | null {
  if (used == null || !Number.isFinite(used)) return null;
  const bounded = Math.max(0, Math.min(100, used));
  return display === "remaining" ? 100 - bounded : bounded;
}

export function quotaPercentLabel(
  used: number | null | undefined,
  display: QuotaDisplay = "consumed",
): string {
  const value = quotaPercent(used, display);
  return value == null ? "—" : `${Math.round(value)}%`;
}

export function quotaDisplayLabel(
  display: QuotaDisplay = "consumed",
  language: Language = "en",
): string {
  return display === "remaining"
    ? language === "zh"
      ? "剩余额度"
      : "Quota remaining"
    : language === "zh"
      ? "已用额度"
      : "Quota used";
}

const text = (language: Language, zh: string, en: string) =>
  language === "zh" ? zh : en;

// Shared Intl formatters, one per locale and options. Building one costs about
// a millisecond, and each Intl.DateTimeFormat built in macOS 26's WebKit leaks a
// little ICU memory (an enumeration JavaScriptCore never closes), so a HUD that
// redraws must not build them per call.
const dateFormats = new Map<string, Intl.DateTimeFormat>();
const numberFormats = new Map<string, Intl.NumberFormat>();

/** A shared Intl.DateTimeFormat; no locale is this machine's. */
export function dateFormat(
  locale?: string,
  options: Intl.DateTimeFormatOptions = {},
): Intl.DateTimeFormat {
  const key = JSON.stringify([locale ?? "", options]);
  let format = dateFormats.get(key);
  if (!format)
    dateFormats.set(key, (format = new Intl.DateTimeFormat(locale, options)));
  return format;
}

/** A shared Intl.NumberFormat; no locale is this machine's. */
export function numberFormat(
  locale?: string,
  options: Intl.NumberFormatOptions = {},
): Intl.NumberFormat {
  const key = JSON.stringify([locale ?? "", options]);
  let format = numberFormats.get(key);
  if (!format)
    numberFormats.set(key, (format = new Intl.NumberFormat(locale, options)));
  return format;
}

/** The fields Date.toLocaleString writes, for a shared formatter that writes the same. */
export const localeDateTime: Intl.DateTimeFormatOptions = {
  year: "numeric",
  month: "numeric",
  day: "numeric",
  hour: "numeric",
  minute: "numeric",
  second: "numeric",
};

let zone = { name: "", readAt: -Infinity };

/** This machine's time zone, read again at most once a minute: it changes as the Mac travels. */
export function systemTimeZone(): string {
  const now = Date.now();
  if (now - zone.readAt >= 60_000)
    zone = {
      name: new Intl.DateTimeFormat().resolvedOptions().timeZone,
      readAt: now,
    };
  return zone.name;
}

/**
 * The exact decimal value of a double. `Intl` rounds a number from its
 * shortest decimal form ("1.05"), printf from its binary value
 * (1.0500000000000000444…), so 1,050 tokens read 1.0k instead of 1.1k.
 */
function exactDecimal(value: number) {
  if (!Number.isFinite(value)) return String(value);
  const view = new DataView(new ArrayBuffer(8));
  view.setFloat64(0, value);
  const bits = view.getBigUint64(0);
  const sign = bits >> 63n ? "-" : "";
  const biased = Number((bits >> 52n) & 0x7ffn);
  let mantissa = bits & ((1n << 52n) - 1n);
  if (biased !== 0) mantissa |= 1n << 52n;
  const exponent = (biased === 0 ? 1 : biased) - 1075;
  if (exponent >= 0) return sign + (mantissa << BigInt(exponent)).toString();
  const scale = -exponent;
  const digits = (mantissa * 5n ** BigInt(scale))
    .toString()
    .padStart(scale + 1, "0");
  return `${sign}${digits.slice(0, -scale)}.${digits.slice(-scale)}`;
}

/** printf's "%.Nf": the exact binary value, ties to even. */
function fixed(value: number, digits: number) {
  return numberFormat("en-US", {
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
    roundingMode: "halfEven",
    useGrouping: false,
  }).format(exactDecimal(value) as never);
}

/** One decimal below ten, none from ten up. */
function trim(value: number) {
  const one = fixed(value, 1);
  return value >= 10 || Number(one) >= 10 ? fixed(value, 0) : one;
}

const tokenUnits = [
  [1_000_000_000, "B"],
  [1_000_000, "M"],
  [1_000, "k"],
] as const;

/**
 * 48_000 → "48k", 1_260 → "1.3k", 950 → "950", 2_400_000 → "2.4M".
 * Better than Open: a value that rounds up to the next unit reads in it
 * ("1.0M", not "1000k"), and 9_999 reads "10k", not "10.0k".
 */
export function tokensShort(tokens: number) {
  for (let index = 0; index < tokenUnits.length; index++) {
    const [size, unit] = tokenUnits[index];
    if (tokens < size) continue;
    const value = trim(tokens / size);
    if (index > 0 && Number(value) >= 1000) {
      const [larger, largerUnit] = tokenUnits[index - 1];
      return trim(tokens / larger) + largerUnit;
    }
    return value + unit;
  }
  return String(Math.trunc(tokens));
}

/** "48k ↓ 12k ↑" */
export function tokensInOut(input: number, output: number) {
  return `${tokensShort(input)} ↓ ${tokensShort(output)} ↑`;
}

/** Half away from zero, as Swift's `rounded()`. */
function roundHalfAway(value: number) {
  return Math.sign(value) * Math.round(Math.abs(value));
}

/** Percent with no decimals: 72.4 → "72%". */
export function percent(value: number) {
  return `${roundHalfAway(value)}%`;
}

/** Percent with one decimal: 6.2 → "6.2%". */
export function percent1(value: number) {
  return `${fixed(value, 1)}%`;
}

/** A part of a whole; a part that rounds to nothing but is not nothing reads "<1%". */
export function share(part: number, whole: number) {
  const value = (part / Math.max(1, whole)) * 100;
  return part > 0 && value < 0.5 ? "<1%" : percent(value);
}

// Money

type Amount = number | string;

function currency(
  language: Language,
  code: string,
  digits: [number, number],
  rounding: "halfEven" | "halfExpand",
) {
  return numberFormat(language === "zh" ? "zh-CN" : "en-US", {
    style: "currency",
    currency: code,
    currencyDisplay: "narrowSymbol",
    minimumFractionDigits: digits[0],
    maximumFractionDigits: digits[1],
    roundingMode: rounding,
  });
}

/** Formats a decimal string exactly; a number as its binary value. */
function formatAmount(format: Intl.NumberFormat, amount: Amount) {
  return format.format(amount as never);
}

const isPositive = (amount: Amount) => Number(amount) > 0;

/** A balance or an estimate with the currency's symbol: "$12.99", or up to six decimals for an estimate. */
export function moneyAmount(
  amount: Amount,
  code: string,
  language: Language,
  estimated = false,
) {
  const format = currency(language, code, [2, estimated ? 6 : 2], "halfEven");
  if (estimated && isPositive(amount) && Number(amount) < 0.000001) {
    return "<" + formatAmount(format, "0.000001");
  }
  return formatAmount(format, amount);
}

/** A list price as a pricing page writes it: whole units for a whole amount ("$20"), else two decimals ("$19.99"). */
export function listPrice(amount: number, code: string, language: Language) {
  const digits = Number.isInteger(amount) ? 0 : 2;
  return formatAmount(
    currency(language, code, [digits, digits], "halfEven"),
    amount,
  );
}

/** Whole units from a hundred up ("$412"), two decimals below ("$3.10"), "<$0.01" for a positive amount that rounds to none. */
export function moneyShort(amount: Amount, code: string, language: Language) {
  const cents = numberFormat("en-US", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
    roundingMode: "halfEven",
    useGrouping: false,
  }).format(amount as never);
  if (Number(cents) === 0 && isPositive(amount)) {
    return (
      "<" + formatAmount(currency(language, code, [2, 2], "halfEven"), "0.01")
    );
  }
  const digits = Math.abs(Number(cents)) >= 100 ? 0 : 2;
  return formatAmount(
    currency(language, code, [digits, digits], "halfEven"),
    cents,
  );
}

/**
 * What some usage would cost at list prices, each currency as it was priced: "≈$12.30 · ≈¥45.60", US dollars first,
 * then by code (Open's ModelCatalog.ListCost.text). Empty when nothing was priced.
 */
export function listCost(amounts: Record<string, Amount>, language: Language) {
  const order = (a: string, b: string) =>
    a === b ? 0 : a === "USD" ? -1 : b === "USD" ? 1 : a < b ? -1 : 1;
  return Object.keys(amounts)
    .sort(order)
    .map((code) => {
      try {
        return "≈" + moneyAmount(amounts[code], code, language);
      } catch {
        // A code Intl does not know still reads as an amount.
        return "≈" + moneyPrice(amounts[code], code);
      }
    })
    .join(" · ");
}

/** A price as the website writes it in both languages: "USD 2.99". */
export function moneyPrice(amount: Amount, code: string) {
  const negative = Number(amount) < 0;
  const absolute =
    typeof amount === "string" ? amount.replace(/^-/, "") : Math.abs(amount);
  const number = numberFormat("en-US", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
    roundingMode: "halfExpand",
  }).format(absolute as never);
  return `${negative ? "-" : ""}${code} ${number}`;
}

// Durations

/** "2h 14m", "4h 02m", "51m". Never negative. */
export function countdown(seconds: number) {
  const total = Math.max(0, roundHalfAway(seconds));
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  return hours > 0
    ? `${hours}h ${String(minutes).padStart(2, "0")}m`
    : `${minutes}m`;
}

/** "2h14m", "51m". */
export function countdownCompact(seconds: number) {
  return countdown(seconds).replaceAll(" ", "");
}

/** Like `countdown`, without a zero minute part: "2h". */
export function countdownRough(seconds: number) {
  const total = Math.max(0, roundHalfAway(seconds));
  if (total >= 3600 && Math.floor((total % 3600) / 60) === 0)
    return `${Math.floor(total / 3600)}h`;
  return countdown(seconds);
}

/** How long ago, in its largest unit: "5m ago" / "5m 前". */
export function age(seconds: number, language: Language) {
  const whole = Math.max(0, Math.trunc(seconds));
  const value =
    whole < 60
      ? `${whole}s`
      : whole < 3600
        ? `${Math.floor(whole / 60)}m`
        : whole < 86400
          ? `${Math.floor(whole / 3600)}h`
          : `${Math.floor(whole / 86400)}d`;
  return text(language, `${value} 前`, `${value} ago`);
}

/** A wait in seconds while that is the honest unit, then compact: "42s", "3m", "1h05m". */
export function waited(seconds: number) {
  const value = Math.max(0, seconds);
  return value < 60 ? `${Math.trunc(value)}s` : countdownCompact(value);
}

/** A forecast in whole minutes rounded up: "2h 14m" / "2小时14分", "2h" / "2小时". */
export function forecast(seconds: number, language: Language) {
  const minutes = Math.min(Math.ceil(seconds / 60), 2_147_483_647);
  const hours = Math.floor(minutes / 60);
  if (hours > 0) {
    if (minutes % 60 === 0) return text(language, `${hours}小时`, `${hours}h`);
    return text(
      language,
      `${hours}小时${minutes % 60}分`,
      `${hours}h ${minutes % 60}m`,
    );
  }
  return text(language, `${minutes}分`, `${minutes}m`);
}

// Calendar

/** A calendar for weekday and week arithmetic: a time zone and the first day of the week (1 = Sunday … 7 = Saturday). */
export interface WeekCalendar {
  timeZone: string;
  firstWeekday: number;
}

/** What every formatter needs from the host: the interface language and the calendar. */
export interface FormatSettings {
  quotaDisplay?: QuotaDisplay;
  language: Language;
  calendar: WeekCalendar;
}

const weekdays: Record<Language, string[]> = {
  zh: ["周日", "周一", "周二", "周三", "周四", "周五", "周六"],
  en: ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"],
};
const months = [
  "Jan",
  "Feb",
  "Mar",
  "Apr",
  "May",
  "Jun",
  "Jul",
  "Aug",
  "Sep",
  "Oct",
  "Nov",
  "Dec",
];

/** The wall-clock fields of an instant in a time zone; weekday 1 = Sunday. */
function wallClock(instant: number, timeZone: string) {
  const parts = Object.fromEntries(
    dateFormat("en-US", {
      timeZone,
      year: "numeric",
      month: "numeric",
      day: "numeric",
      hour: "numeric",
      minute: "numeric",
      weekday: "short",
      hourCycle: "h23",
    })
      .formatToParts(instant)
      .map((part) => [part.type, part.value]),
  );
  const year = Number(parts.year),
    month = Number(parts.month),
    day = Number(parts.day);
  return {
    year,
    month,
    day,
    hour: Number(parts.hour),
    minute: Number(parts.minute),
    weekday: weekdays.en.indexOf(parts.weekday) + 1,
    /** Days since 1970-01-01 of this wall-clock date. */
    dayNumber: Math.floor(Date.UTC(year, month - 1, day) / 86_400_000),
  };
}

const two = (value: number) => String(value).padStart(2, "0");

/** "周日 02:00" / "Sun 02:00". */
function weekdayTime(
  instant: number,
  calendar: WeekCalendar,
  language: Language,
) {
  const clock = wallClock(instant, calendar.timeZone);
  return `${weekdays[language][clock.weekday - 1]} ${two(clock.hour)}:${two(clock.minute)}`;
}

/** "11月20日 周五 09:00" / "Fri 20 Nov at 09:00". */
function dateTime(instant: number, calendar: WeekCalendar, language: Language) {
  const clock = wallClock(instant, calendar.timeZone);
  const time = `${two(clock.hour)}:${two(clock.minute)}`;
  const weekday = weekdays[language][clock.weekday - 1];
  return language === "zh"
    ? `${clock.month}月${clock.day}日 ${weekday} ${time}`
    : `${weekday} ${clock.day} ${months[clock.month - 1]} at ${time}`;
}

/** "10月28日 16:10" / "Oct 28, 16:10": a time a week or more away. */
export function monthDayTime(
  instant: number,
  calendar: WeekCalendar,
  language: Language,
) {
  const clock = wallClock(instant, calendar.timeZone);
  const time = `${two(clock.hour)}:${two(clock.minute)}`;
  return language === "zh"
    ? `${clock.month}月${clock.day}日 ${time}`
    : `${months[clock.month - 1]} ${clock.day}, ${time}`;
}

/** Whole calendar weeks between the weeks containing `from` and `to`. */
function weeksBetween(
  from: number,
  to: number,
  timeZone: string,
  firstWeekday: number,
) {
  const start = (instant: number) => {
    const clock = wallClock(instant, timeZone);
    return clock.dayNumber - ((clock.weekday - firstWeekday + 7) % 7);
  };
  return Math.trunc((start(to) - start(from)) / 7);
}

/**
 * A future date's weekday with which week it falls in: "下周二 16:10" / "Next Tue 16:10". The current week needs no
 * marker; Chinese counts Monday-started weeks up to three "下", English names the next week only; anything further
 * names its date.
 */
export function relativeWeekdayTime(
  instant: number,
  now: number,
  calendar: WeekCalendar,
  language: Language,
) {
  const firstWeekday = language === "zh" ? 2 : calendar.firstWeekday;
  const delta = weeksBetween(now, instant, calendar.timeZone, firstWeekday);
  if (delta <= 0) return weekdayTime(instant, calendar, language);
  if (delta <= 3) {
    if (language === "zh")
      return "下".repeat(delta) + weekdayTime(instant, calendar, language);
    if (delta === 1) return `Next ${weekdayTime(instant, calendar, language)}`;
  }
  return dateTime(instant, calendar, language);
}

/** A countdown inside 24 hours ("3h 10m"), else the weekday and time; "—" when unknown. */
export function resetLabel(
  instant: number | null,
  now: number,
  calendar: WeekCalendar,
  language: Language,
) {
  if (instant === null) return "—";
  const seconds = (instant - now) / 1000;
  return seconds < 86_400
    ? countdown(seconds)
    : relativeWeekdayTime(instant, now, calendar, language);
}

/** The menu's variant: "3h10m" or the weekday and time. */
export function resetLabelCompact(
  instant: number | null,
  now: number,
  calendar: WeekCalendar,
  language: Language,
) {
  if (instant === null) return "—";
  const seconds = (instant - now) / 1000;
  return seconds < 86_400
    ? countdownCompact(seconds)
    : relativeWeekdayTime(instant, now, calendar, language);
}

// Quota outlook

export type OutlookKind =
  | "untimed"
  | "exhausted"
  | "no-estimate"
  | "no-usage"
  | "insufficient-data"
  | "exhausts";

/** The share of the window used by its reset: "85% by reset" / "重置时 85%". */
export function byReset(projectedUsedPercent: number, language: Language) {
  const value = roundHalfAway(projectedUsedPercent);
  return text(language, `重置时 ${value}%`, `${value}% by reset`);
}

/**
 * A window's outlook in words: nothing for a window without a reset, how long the rest lasts for one that runs out
 * before its reset, and the share its pace uses by the reset for one that runs out after it.
 */
export function outlookText(
  outlook: { kind: OutlookKind; seconds?: number; beforeReset?: boolean },
  projectedUsedAtReset: number | null,
  language: Language,
): string | null {
  switch (outlook.kind) {
    case "untimed":
      return null;
    case "exhausted":
      return text(language, "已耗尽", "Exhausted");
    case "no-estimate":
      return text(language, "暂无预测", "No estimate");
    case "no-usage":
      return text(language, "暂无消耗", "No usage");
    case "insufficient-data":
      return text(language, "记录不足", "Insufficient data");
    case "exhausts": {
      if (outlook.beforeReset === false) {
        return projectedUsedAtReset === null
          ? text(language, "记录不足", "Insufficient data")
          : byReset(projectedUsedAtReset, language);
      }
      const exhaustion = forecast(outlook.seconds ?? 0, language);
      return text(language, `耗尽 ~${exhaustion}`, `Exhausts ~${exhaustion}`);
    }
  }
}

/** What a forecast's pace comes from: this cycle's recent readings, or the cycle so far on a straight line. */
export function outlookBasisText(
  basis: "recent" | "window",
  language: Language,
) {
  return basis === "window"
    ? text(language, "按整个窗口估算", "Estimated from the whole window")
    : text(language, "按目前节奏", "At the recent pace");
}

/** Where a window's use stands against a steady pace through its cycle: "12 pts ahead of budget" / "比预算超前 12 个点". */
export function budgetText(
  budget: { paceDelta: number; status: "ahead" | "behind" | "onTrack" },
  language: Language,
) {
  const points = roundHalfAway(Math.abs(budget.paceDelta));
  const unit = points === 1 ? "pt" : "pts";
  switch (budget.status) {
    case "ahead":
      return text(
        language,
        `比预算超前 ${points} 个点`,
        `${points} ${unit} ahead of budget`,
      );
    case "behind":
      return text(
        language,
        `比预算少用 ${points} 个点`,
        `${points} ${unit} under budget`,
      );
    case "onTrack":
      return text(language, "与预算持平", "On budget");
  }
}

/** The burn rate's detail while readings are still too few: first sampling or again after a reset. */
export function samplingLabel(afterReset: boolean, language: Language) {
  return afterReset
    ? text(language, "重新采样", "Resampling")
    : text(language, "采样中", "Sampling");
}

/** The sampling hint: why, then the span observed so far beside the span needed. */
export function samplingHint(
  elapsed: number,
  required: number,
  afterReset: boolean,
  language: Language,
) {
  const reason = afterReset
    ? text(
        language,
        "额度已重置，正在重新采样",
        "Quota reset; collecting new readings",
      )
    : text(
        language,
        "首次采样，正在积累预测数据",
        "Collecting initial readings for a forecast",
      );
  // A partial minute must not claim the minimum has been met.
  const observed =
    elapsed < 60
      ? text(language, "不足1分", "less than 1m")
      : forecast(Math.floor(elapsed / 60) * 60, language);
  const minimum = forecast(required, language);
  return `${reason}\n${text(language, `已采样 ${observed}，预测至少需要${minimum}的有效记录`, `Sampled ${observed}; at least ${minimum} of readings needed`)}`;
}

// The host

/** The interface language for a setting: "zh…" is Chinese, "system" follows the system's first language. */
export function resolveLanguage(
  setting: string | null | undefined,
  system: string,
): Language {
  const language = !setting || setting === "system" ? system : setting;
  return language.toLowerCase().startsWith("zh") ? "zh" : "en";
}

/** The interface language of a snapshot: the one Core resolved and writes
 * its own text in, else the setting as this web view resolves it. */
export function snapshotLanguage(
  snapshot:
    { language?: string; settings: { language: string } } | null | undefined,
): Language {
  if (snapshot?.language === "zh" || snapshot?.language === "en")
    return snapshot.language;
  return interfaceLanguage(snapshot?.settings.language);
}

/** The interface language for a setting on this machine. */
export function interfaceLanguage(
  setting: string | null | undefined,
): Language {
  return resolveLanguage(
    setting,
    typeof navigator === "undefined" ? "en-US" : navigator.language,
  );
}

const firstDays = new Map<string, number>();

/** This machine's time zone and first day of the week (1 = Sunday … 7 = Saturday), as its locale has it. */
export function systemCalendar(locale: string): WeekCalendar {
  let firstDay = firstDays.get(locale);
  if (firstDay === undefined) {
    type WeekInfo = { firstDay: number };
    const info = new Intl.Locale(locale) as Intl.Locale & {
      getWeekInfo?: () => WeekInfo;
      weekInfo?: WeekInfo;
    };
    firstDay = info.getWeekInfo?.().firstDay ?? info.weekInfo?.firstDay ?? 1;
    firstDays.set(locale, firstDay);
  }
  return { timeZone: systemTimeZone(), firstWeekday: (firstDay % 7) + 1 };
}
