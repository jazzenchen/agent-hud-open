// What an account's section says in words: whether it is current or when it
// was last read, and when its earned resets expire. Kept apart from the
// components so the tests can run them.
import type { Account, Provider, ResetCredits } from "@agent-hud/protocol";
import {
  countdownRough,
  dateFormat,
  locale,
  translate,
  type Language,
} from "@agent-hud/ui/format";

/** Whether the account is the one signed in, or when it was last read
 * (Open's ReadingAssessment.accountLabel). */
export function accountReadLabel(
  account: Account,
  now: number,
  language: Language,
) {
  if (account.current !== false && !account.stale)
    return translate(language, "Current account");
  const checked = account.checkedAt ? Date.parse(account.checkedAt) : NaN;
  const ago = countdownRough(
    Math.max(0, (now - (Number.isFinite(checked) ? checked : now)) / 1000),
  );
  return translate(language, "Last read {{ago}} ago", { ago });
}

/** What the vendor's status page says about the components the agent
 * depends on, while one is not operational: "官方状态：部分故障 · <incident>".
 * Null while all is well. */
export function serviceNotice(
  providers: readonly Pick<Provider, "id" | "accountStatus">[],
  vendor: string,
  language: Language,
): string | null {
  const service = providers.find((provider) => provider.id === vendor)
    ?.accountStatus?.service;
  if (!service) return null;
  const words: Record<string, string> = {
    under_maintenance: translate(language, "Vendor status: under maintenance"),
    degraded_performance: translate(
      language,
      "Vendor status: degraded performance",
    ),
    partial_outage: translate(language, "Vendor status: partial outage"),
    major_outage: translate(language, "Vendor status: major outage"),
  };
  const head =
    words[service.status] ??
    translate(language, "Vendor status: {{status}}", {
      status: service.status,
    });
  return service.incident ? `${head} · ${service.incident}` : head;
}

/** Why a detected agent's block has no reading, in its reader's words: a
 * client signed out, a login without plan limits, a read that failed. Null
 * when the reader said nothing. A block without readings is named by its
 * provider's id, so only that provider speaks for it. */
export function missingReadingNotice(
  providers: readonly Pick<Provider, "id" | "notice">[],
  vendor: string,
): string | null {
  const notice = providers.find((provider) => provider.id === vendor)?.notice;
  return notice?.trim() ? notice : null;
}

/** Where the block offers a sign-in for its quota: the provider and the
 * sites to sign in on, while no site has a session or the one kept ended
 * (the reading says the agent signed out). Null when nothing is offered. */
export function webSignInOffer(
  providers: readonly Pick<Provider, "id" | "accountStatus">[],
  vendor: string,
): { provider: string; sites: { host: string; name: string }[] } | null {
  const status = providers.find((p) => p.id === vendor)?.accountStatus;
  const web = status?.webSignIn;
  if (!web?.sites.length) return null;
  const ended = status?.failure === "signedOut";
  if (!ended && web.sites.some((site) => site.signedIn)) return null;
  const sites = ended ? web.sites.filter((site) => site.signedIn) : web.sites;
  return {
    provider: web.provider,
    sites: (sites.length ? sites : web.sites).map(({ host, name }) => ({
      host,
      name,
    })),
  };
}

/** The earliest expiry, whole and short (Open's CodexResetCreditsView
 * nextExpiryLabel): the short form is the date alone; a label without a
 * date has none shorter than itself. */
export function nextExpiryLabel(resets: ResetCredits, language: Language) {
  const dates = (resets.credits ?? [])
    .map((credit) => (credit.expiresAt ? new Date(credit.expiresAt) : null))
    .filter(
      (date): date is Date => date !== null && Number.isFinite(date.getTime()),
    );
  if (!dates.length) {
    const label = translate(language, "Expiry unavailable");
    return { full: label, short: label };
  }
  if (dates.length !== resets.availableCount) {
    const label = translate(language, "Partial expiry details");
    return { full: label, short: label };
  }
  const day = dateFormat(locale(language), {
    month: "short",
    day: "numeric",
  }).format(dates[0]);
  return {
    full: translate(language, "First expires {{day}}", { day }),
    short: day,
  };
}

/** A credit's expiry to the minute, in local time (ResetCreditsDetails.expiryLabel). */
export function expiryLabel(
  expiresAt: string | null | undefined,
  language: Language,
) {
  const date = expiresAt ? new Date(expiresAt) : null;
  if (!date || !Number.isFinite(date.getTime()))
    return translate(language, "Expiry unavailable");
  return dateFormat(locale(language), {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    hourCycle: "h23",
  }).format(date);
}
