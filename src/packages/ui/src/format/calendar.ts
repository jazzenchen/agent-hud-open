// Calendar: weekdays, dates and reset times, and this machine's week.
import { countdown, countdownCompact } from "./durations.ts";
import { dateFormat, systemTimeZone } from "./intl.ts";
import { isChinese, locale, translate, type Language } from "./language.ts";
import type { QuotaDisplay } from "./quota-text.ts";

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

const englishWeekdays = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const weekdayNames = new Map<Language, string[]>();

/** The short weekday names of a language, Sunday first. */
function weekdays(language: Language): string[] {
  if (language === "en") return englishWeekdays;
  let names = weekdayNames.get(language);
  if (!names) {
    const format = dateFormat(locale(language), {
      weekday: "short",
      timeZone: "UTC",
    });
    // 2023-01-01 was a Sunday.
    names = englishWeekdays.map((_, day) =>
      format.format(Date.UTC(2023, 0, 1 + day)),
    );
    weekdayNames.set(language, names);
  }
  return names;
}
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
    weekday: englishWeekdays.indexOf(parts.weekday) + 1,
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
  return `${weekdays(language)[clock.weekday - 1]} ${two(clock.hour)}:${two(clock.minute)}`;
}

/** "11月20日 周五 09:00" / "Fri 20 Nov at 09:00". */
function dateTime(instant: number, calendar: WeekCalendar, language: Language) {
  const clock = wallClock(instant, calendar.timeZone);
  const time = `${two(clock.hour)}:${two(clock.minute)}`;
  const weekday = weekdays(language)[clock.weekday - 1];
  if (isChinese(language))
    return `${clock.month}月${clock.day}日 ${weekday} ${time}`; // l10n-ignore
  if (language === "en")
    return `${weekday} ${clock.day} ${months[clock.month - 1]} at ${time}`;
  return dateFormat(locale(language), {
    timeZone: calendar.timeZone,
    weekday: "short",
    day: "numeric",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
    hourCycle: "h23",
  }).format(instant);
}

/** "10月28日 16:10" / "Oct 28, 16:10": a time a week or more away. */
export function monthDayTime(
  instant: number,
  calendar: WeekCalendar,
  language: Language,
) {
  const clock = wallClock(instant, calendar.timeZone);
  const time = `${two(clock.hour)}:${two(clock.minute)}`;
  if (isChinese(language)) return `${clock.month}月${clock.day}日 ${time}`; // l10n-ignore
  if (language === "en")
    return `${months[clock.month - 1]} ${clock.day}, ${time}`;
  return dateFormat(locale(language), {
    timeZone: calendar.timeZone,
    day: "numeric",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
    hourCycle: "h23",
  }).format(instant);
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
  const firstWeekday = isChinese(language) ? 2 : calendar.firstWeekday;
  const delta = weeksBetween(now, instant, calendar.timeZone, firstWeekday);
  if (delta <= 0) return weekdayTime(instant, calendar, language);
  if (delta <= 3) {
    if (isChinese(language))
      return (
        "下".repeat(delta) + // l10n-ignore: next week
        weekdayTime(instant, calendar, language)
      );
    if (delta === 1)
      return translate(language, "Next {{weekdayTime}}", {
        weekdayTime: weekdayTime(instant, calendar, language),
      });
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
