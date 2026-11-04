// Durations
import { text, type Language } from "./language.ts";
import { roundHalfAway } from "./numbers.ts";

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
