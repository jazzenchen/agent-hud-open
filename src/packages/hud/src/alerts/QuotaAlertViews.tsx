import type { IslandEvent, Snapshot } from "@agent-hud/protocol";
import { agentVendorName } from "../model";
import { alertFill, alertInk, alertText, onAlertFill } from "../tokens";
import { useGround } from "../ground";
import { cardColumn } from "../card-column";
import { palette } from "../panel/theme";
import { useFormat } from "@agent-hud/ui/format-context";
import { quotaAlertCopy, resetCreditCopy } from "./quota-alert-copy";
import { Heading, Metric, StatsButton } from "./AlertCardParts";
import { useT } from "@agent-hud/ui/format-context";

/** Usage resets added to an account (Open's ResetCreditAlertDetailView): the
 * client and the account, what was added, how many the account holds and its
 * first expiry as a date. */
export function ResetCreditAlertView({
  event,
  snapshot,
  dismiss,
  onStats,
  light = false,
  botLogos = true,
}: {
  event: IslandEvent;
  snapshot?: Snapshot;
  dismiss: () => void;
  onStats?: () => void;
  light?: boolean;
  botLogos?: boolean;
}) {
  const t = useT();
  const { language } = useFormat();
  const ground = useGround(light);
  const ink = (alpha: number) => alertInk(alpha, ground);
  const account = snapshot?.accounts?.find(
    (candidate) => candidate.id === event.accountId,
  );
  const added = event.added ?? 1;
  const available =
    account?.resetCredits?.availableCount ?? event.availableCount ?? added;
  // Every reset the account holds, as the usage panel lists them; else the new ones.
  const expiries =
    account?.resetCredits?.credits?.map((credit) => credit.expiresAt) ??
    (Array.isArray(event.newCredits)
      ? event.newCredits.map((credit) => credit?.expiresAt)
      : []);
  const copy = resetCreditCopy(added, expiries, language);
  return (
    <article style={{ ...cardColumn(20), fontSize: 12, color: "inherit" }}>
      <Heading
        event={event}
        name={agentVendorName(event.source)}
        under={account?.name ?? event.title ?? null}
        news={t("new reset")}
        accentFill={alertFill("calm", ground)}
        accentText={alertText("calm", ground)}
        light={light}
        botLogos={botLogos}
      />
      <div style={{ fontSize: 12, color: ink(0.58) }}>{copy.title}</div>
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(3, minmax(0, 1fr))",
          gap: 12,
        }}
      >
        <Metric value={`+${added}`} label={t("added")} light={light} />
        <Metric value={`${available}`} label={t("available")} light={light} />
        <Metric value={copy.expiry} label={t("first expires")} light={light} />
      </div>
      <StatsButton
        fill={alertFill("calm", ground)}
        light={light}
        onClick={() => {
          dismiss();
          onStats?.();
        }}
      />
    </article>
  );
}

/** An agent that signed out: its name, what its account reading says (how
 * to sign in again), and that its last quota stays on screen until then.
 * Told on the island only, at most once a day. */
export function SignInAlertView({
  event,
  dismiss,
  light = false,
  botLogos = true,
}: {
  event: IslandEvent;
  dismiss: () => void;
  light?: boolean;
  botLogos?: boolean;
}) {
  const t = useT();
  const ground = useGround(light);
  const ink = (alpha: number) => alertInk(alpha, ground);
  const name = event.title ?? agentVendorName(event.source);
  return (
    <article style={{ ...cardColumn(14), fontSize: 12, color: "inherit" }}>
      <Heading
        event={event}
        name={name}
        under={null}
        news={t("sign in again")}
        accentFill={alertFill("warning", ground)}
        accentText={alertText("warning", ground)}
        light={light}
        botLogos={botLogos}
      />
      {event.message && (
        <div style={{ fontSize: 12, lineHeight: 1.45, color: ink(0.78) }}>
          {event.message}
        </div>
      )}
      <div style={{ fontSize: 10, color: ink(0.42) }}>
        {t("Until then, its quota shows the last reading.")}
      </div>
      <button
        type="button"
        onClick={dismiss}
        style={{
          height: 30,
          width: "100%",
          borderRadius: 7,
          border: 0,
          background: alertFill("warning", ground),
          color: onAlertFill({ light }),
          opacity: 0.9,
          fontSize: 12,
          fontWeight: 600,
          cursor: "pointer",
        }}
      >
        {t("OK [acknowledge]")}
      </button>
    </article>
  );
}

/** A quota window's news (Open's QuotaAlertDetailView): the agent with the
 * window under it, what happened in a sentence, the share used, how long
 * until empty (or what is left after a reset) and the next reset, the
 * window's bar, any other windows still empty, and the way to statistics. */
export function QuotaAlertView({
  event,
  dismiss,
  onStats,
  light = false,
  botLogos = true,
}: {
  event: IslandEvent;
  dismiss: () => void;
  onStats?: () => void;
  light?: boolean;
  botLogos?: boolean;
}) {
  const t = useT();
  const ground = useGround(light);
  const ink = (alpha: number) => alertInk(alpha, ground);
  const settings = useFormat();
  const copy = quotaAlertCopy(event, settings);
  const isReset = event.kind === "quota-reset";
  const tone = isReset ? "calm" : "warning";
  const accentFill = alertFill(tone, ground);
  const accentText = alertText(tone, ground);
  const windowName = event.title ?? "";
  return (
    <article style={{ ...cardColumn(20), fontSize: 12, color: "inherit" }}>
      <Heading
        event={event}
        name={agentVendorName(event.source)}
        under={windowName || null}
        news={isReset ? t("reset") : t("running low")}
        accentFill={accentFill}
        accentText={accentText}
        light={light}
        botLogos={botLogos}
      />
      <div style={{ fontSize: 12, color: ink(0.58) }}>{copy.title}</div>
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(3, minmax(0, 1fr))",
          gap: 12,
        }}
      >
        <Metric value={copy.display} label={copy.displayLabel} light={light} />
        <Metric
          value={
            isReset
              ? settings.quotaDisplay === "remaining"
                ? copy.used
                : copy.remaining
              : copy.exhaustion
          }
          label={
            isReset
              ? settings.quotaDisplay === "remaining"
                ? t("used")
                : t("remaining")
              : t("until empty")
          }
          light={light}
        />
        <Metric value={copy.reset} label={t("next reset")} light={light} />
      </div>
      <div style={cardColumn(7)}>
        <div style={{ display: "flex", fontSize: 10, color: ink(0.42) }}>
          <span
            style={{
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
            }}
          >
            {windowName}
          </span>
          <span style={{ flex: 1, minWidth: 8 }} />
          <span style={{ fontVariantNumeric: "tabular-nums" }}>
            {copy.display}
          </span>
        </div>
        <div
          style={{
            height: 4,
            borderRadius: 4,
            background: palette(ground).track,
            overflow: "hidden",
          }}
        >
          <div
            style={{
              width: `${copy.displayPct ?? 0}%`,
              height: "100%",
              borderRadius: 4,
              background: accentFill,
            }}
          />
        </div>
        {copy.otherLimits && (
          <span style={{ fontSize: 10, color: accentText }}>
            {copy.otherLimits}
          </span>
        )}
      </div>
      <StatsButton
        fill={accentFill}
        light={light}
        onClick={() => {
          dismiss();
          onStats?.();
        }}
      />
    </article>
  );
}
