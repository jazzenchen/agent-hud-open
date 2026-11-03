import { ArrowUpRight } from "lucide-react";
import type { IslandEvent, Snapshot } from "@agent-hud/protocol";
import { agentVendorName } from "../model";
import { ReminderMark } from "./ReminderMark";
import { alertFill, alertInk, alertText, onAlertFill } from "../tokens";
import { useGround } from "../ground";
import { cardColumn } from "../card-column";
import { palette } from "../panel/theme";
import { MiddleTruncated } from "../panel/MiddleTruncated";
import { useFormat } from "@agent-hud/ui/format-context";
import { quotaAlertCopy, resetCreditCopy } from "./quota-alert-copy";

/** A card's three figures, each over its label (23 pt medium figures, 10 pt labels). */
function Metric({
  value,
  label,
  light,
}: {
  value: string;
  label: string;
  light: boolean;
}) {
  const ink = (alpha: number) => alertInk(alpha, { light });
  return (
    <div style={{ ...cardColumn(4), minWidth: 0 }}>
      <span
        style={{
          fontSize: 23,
          fontWeight: 500,
          fontVariantNumeric: "tabular-nums",
          color: ink(0.96),
          whiteSpace: "nowrap",
          overflow: "hidden",
          textOverflow: "ellipsis",
        }}
      >
        {value}
      </span>
      <span style={{ fontSize: 10, color: ink(0.4) }}>{label}</span>
    </div>
  );
}

/** The card's heading: the mark, the agent with a line under it, and the news in the accent. */
function Heading({
  event,
  name,
  under,
  news,
  accentFill,
  accentText,
  light,
  botLogos,
}: {
  event: IslandEvent;
  name: string;
  under: string | null;
  news: string;
  accentFill: string;
  accentText: string;
  light: boolean;
  botLogos: boolean;
}) {
  const ink = (alpha: number) => alertInk(alpha, { light });
  return (
    <header style={{ display: "flex", alignItems: "center", gap: 9 }}>
      <ReminderMark event={event} size={22} light={light} botLogos={botLogos} />
      <div style={{ ...cardColumn(3), minWidth: 0 }}>
        <strong style={{ fontSize: 13, fontWeight: 600, color: ink(0.94) }}>
          {name}
        </strong>
        {under && (
          <MiddleTruncated
            text={under}
            style={{ fontSize: 10, color: ink(0.42) }}
          />
        )}
      </div>
      <span style={{ flex: 1, minWidth: 8 }} />
      <span
        style={{
          width: 5,
          height: 5,
          borderRadius: "50%",
          background: accentFill,
          flex: "0 0 5px",
        }}
      />
      <span
        style={{
          fontSize: 10,
          fontWeight: 500,
          color: accentText,
          whiteSpace: "nowrap",
        }}
      >
        {news}
      </span>
    </header>
  );
}

/** "View usage statistics", filled with the accent (Open's card button). */
function StatsButton({
  fill,
  light,
  zh,
  onClick,
}: {
  fill: string;
  light: boolean;
  zh: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      style={{
        height: 30,
        width: "100%",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        gap: 6,
        borderRadius: 7,
        border: 0,
        background: fill,
        color: onAlertFill({ light }),
        opacity: 0.9,
        fontSize: 12,
        fontWeight: 600,
        cursor: "pointer",
      }}
    >
      <span>{zh ? "查看用量统计" : "View usage statistics"}</span>
      <ArrowUpRight size={9} strokeWidth={2.6} />
    </button>
  );
}

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
  zh = false,
}: {
  event: IslandEvent;
  snapshot?: Snapshot;
  dismiss: () => void;
  onStats?: () => void;
  light?: boolean;
  botLogos?: boolean;
  zh?: boolean;
}) {
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
  const copy = resetCreditCopy(added, expiries, zh ? "zh" : "en");
  return (
    <article style={{ ...cardColumn(20), fontSize: 12, color: "inherit" }}>
      <Heading
        event={event}
        name={agentVendorName(event.source)}
        under={account?.name ?? event.title ?? null}
        news={zh ? "新增重置" : "new reset"}
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
        <Metric
          value={`+${added}`}
          label={zh ? "新增" : "added"}
          light={light}
        />
        <Metric
          value={`${available}`}
          label={zh ? "可用次数" : "available"}
          light={light}
        />
        <Metric
          value={copy.expiry}
          label={zh ? "最早到期" : "first expires"}
          light={light}
        />
      </div>
      <StatsButton
        fill={alertFill("calm", ground)}
        light={light}
        zh={zh}
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
  zh = false,
}: {
  event: IslandEvent;
  dismiss: () => void;
  light?: boolean;
  botLogos?: boolean;
  zh?: boolean;
}) {
  const ground = useGround(light);
  const ink = (alpha: number) => alertInk(alpha, ground);
  const name = event.title ?? agentVendorName(event.source);
  return (
    <article style={{ ...cardColumn(14), fontSize: 12, color: "inherit" }}>
      <Heading
        event={event}
        name={name}
        under={null}
        news={zh ? "需要重新登录" : "sign in again"}
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
        {zh
          ? "重新登录前，额度显示上次读到的数字。"
          : "Until then, its quota shows the last reading."}
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
        {zh ? "知道了" : "OK"}
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
  zh = false,
}: {
  event: IslandEvent;
  dismiss: () => void;
  onStats?: () => void;
  light?: boolean;
  botLogos?: boolean;
  zh?: boolean;
}) {
  const ground = useGround(light);
  const ink = (alpha: number) => alertInk(alpha, ground);
  const settings = useFormat();
  const copy = quotaAlertCopy(event, {
    ...settings,
    language: zh ? "zh" : "en",
  });
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
        news={
          isReset ? (zh ? "已重置" : "reset") : zh ? "额度预警" : "running low"
        }
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
                ? zh
                  ? "已用额度"
                  : "used"
                : zh
                  ? "剩余额度"
                  : "remaining"
              : zh
                ? "预计耗尽"
                : "until empty"
          }
          light={light}
        />
        <Metric
          value={copy.reset}
          label={zh ? "下次重置" : "next reset"}
          light={light}
        />
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
        zh={zh}
        onClick={() => {
          dismiss();
          onStats?.();
        }}
      />
    </article>
  );
}
