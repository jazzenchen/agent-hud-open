// The pieces the quota, reset and sign-in cards share.
import { ArrowUpRight } from "lucide-react";
import type { IslandEvent } from "@agent-hud/protocol";
import { ReminderMark } from "./ReminderMark";
import { alertInk, onAlertFill } from "../tokens";
import { cardColumn } from "../card-column";
import { MiddleTruncated } from "../panel/MiddleTruncated";
import { useT } from "@agent-hud/ui/format-context";

/** A card's three figures, each over its label (23 pt medium figures, 10 pt labels). */
export function Metric({
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
export function Heading({
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
export function StatsButton({
  fill,
  light,
  onClick,
}: {
  fill: string;
  light: boolean;
  onClick: () => void;
}) {
  const t = useT();
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
      <span>{t("View usage statistics")}</span>
      <ArrowUpRight size={9} strokeWidth={2.6} />
    </button>
  );
}
