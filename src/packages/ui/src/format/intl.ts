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
