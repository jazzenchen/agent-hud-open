// What a quota or reset-credit reminder says (Open's QuotaAlertCopy and
// ResetCreditAlertCopy), kept apart from the cards so the tests can run it.
import type { IslandEvent } from "@agent-hud/protocol";
import {
  countdownCompact,
  dateFormat,
  percent,
  quotaPercent,
  quotaDisplayLabel,
  resetLabelCompact,
  listSeparator,
  locale,
  translate,
  translator,
  type FormatSettings,
  type Language,
} from "@agent-hud/ui/format";

/** The window's share left, from the share used the event carries; none without a reading. */
function remainingOf(event: IslandEvent) {
  return event.usedPercent == null || !Number.isFinite(event.usedPercent)
    ? null
    : Math.max(0, Math.min(100, 100 - event.usedPercent));
}

export function quotaAlertCopy(event: IslandEvent, settings: FormatSettings) {
  const { language } = settings;
  const t = translator(language);
  const remaining = remainingOf(event);
  const usedPct = remaining == null ? null : Math.max(0, 100 - remaining);
  const displayPct = quotaPercent(event.usedPercent, settings.quotaDisplay);
  const isReset = event.kind === "quota-reset";
  // Core marks the reading that empties the window (Open's AlertPolicy.exhaustedRemaining).
  const empty = event.crossing === "exhausted";
  const forecast = event.timeToExhaustSeconds;
  const exhaustion =
    remaining != null && forecast != null && Number.isFinite(forecast)
      ? countdownCompact(Math.max(60, Math.ceil(forecast / 60) * 60))
      : null;
  // The reset is measured from when the window was read, as Open measures it.
  const resetAt = event.resetAt ? Date.parse(event.resetAt) : NaN;
  const readAt = Date.parse(event.at);
  const others = (event.otherExhaustedWindows ?? [])
    .map((window) => window?.name)
    .filter((name): name is string => Boolean(name));
  const names = others.join(listSeparator(language));
  return {
    displayPct,
    display: displayPct == null ? "N/A" : percent(displayPct),
    displayLabel: quotaDisplayLabel(settings.quotaDisplay, language),
    usedPct,
    used: usedPct == null ? "N/A" : percent(usedPct),
    remaining: remaining == null ? "N/A" : percent(remaining),
    exhaustion: exhaustion ?? "—",
    reset: resetLabelCompact(
      Number.isFinite(resetAt) ? resetAt : null,
      Number.isFinite(readAt) ? readAt : Date.now(),
      settings.calendar,
      language,
    ),
    /** Beside the agent in the small reminder. */
    compactTitle:
      remaining == null
        ? t("Quota unavailable")
        : isReset
          ? t("Reset [status]")
          : empty
            ? t("Empty")
            : exhaustion
              ? t("{{exhaustion}} left", { exhaustion })
              : t("Running low"),
    /** The card's sentence. */
    title:
      remaining == null
        ? t("Quota is unavailable.")
        : isReset
          ? others.length
            ? t("This window has reset; other limits still apply.")
            : t("This quota window has reset.")
          : empty
            ? t("This window is empty. Waiting for reset.")
            : exhaustion
              ? t(
                  "At the recent burn rate, this window may run out before it resets.",
                )
              : t("Usage has reached the critical threshold."),
    otherLimits: others.length
      ? t("Still exhausted: {{names}}", { names })
      : null,
  };
}

/** The reset-credit card's words. `expiresAt` are the expiries of every reset
 * the account holds, as the usage panel lists them. */
export function resetCreditCopy(
  added: number,
  expiresAt: (string | null | undefined)[],
  language: Language,
) {
  const first = expiresAt
    .map((at) => (at ? Date.parse(at) : NaN))
    .filter(Number.isFinite)
    .sort((a, b) => a - b)[0];
  return {
    compactTitle: translate(language, "+{{count}} resets", { count: added }),
    title: translate(
      language,
      "{{count}} usage resets were added to this account.",
      { count: added },
    ),
    /** The account's earliest deadline as a date. */
    expiry:
      first === undefined
        ? "—"
        : dateFormat(locale(language), {
            month: "short",
            day: "numeric",
          }).format(first),
  };
}
