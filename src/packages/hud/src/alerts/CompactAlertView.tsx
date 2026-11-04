import {
  Flame,
  Hand,
  KeyRound,
  MessageSquareText,
  RotateCcw,
  ShieldAlert,
} from "lucide-react";
import type { Approval, IslandEvent } from "@agent-hud/protocol";
import { labelWidth } from "../geometry";
import { ReminderMark } from "./ReminderMark";
import { compactAlertLabels } from "./compact-alert-labels";
import { alertInk, alertText } from "../tokens";
import { useGround } from "../ground";

export function compactAlertDockWidth(
  request: Approval | undefined,
  event: IslandEvent | undefined,
  requestCount: number,
  scale: number,
  zh: boolean,
): number {
  const { who, status, badge } = compactAlertLabels(
    request,
    event,
    requestCount,
    zh,
  );
  const font = Math.round(9 * scale);
  const icon = Math.round(17 * scale);
  const glyph = Math.round(9 * scale);
  const leading = icon + Math.round(5 * scale) + labelWidth(who, font);
  const badgeExtra = badge
    ? Math.round(4 * scale) + labelWidth(badge, Math.round(8 * scale)) + 8
    : 0;
  const trailing =
    glyph + Math.round(4 * scale) + labelWidth(status, font) + badgeExtra;
  return Math.ceil(
    leading + Math.round(8 * scale) + trailing + Math.round(20 * scale),
  );
}

export function CompactAlertView({
  request,
  event,
  requestCount,
  onDock,
  notchWidth,
  surfaceFlare,
  scale,
  light,
  botLogos,
  zh,
  onOpen,
  onDismissAndStats,
}: {
  request?: Approval;
  event?: IslandEvent;
  requestCount: number;
  onDock: boolean;
  notchWidth: number;
  surfaceFlare: number;
  scale: number;
  light: boolean;
  botLogos: boolean;
  zh: boolean;
  onOpen: () => void;
  onDismissAndStats: () => void;
}) {
  const { who, status, badge } = compactAlertLabels(
    request,
    event,
    requestCount,
    zh,
  );
  const font = onDock ? Math.round(9 * scale) : 12;
  const iconSize = onDock ? Math.round(17 * scale) : 17;
  const glyphSize = onDock ? Math.round(9 * scale) : 11;
  const ground = useGround(light);
  const signalText = alertText("warning", ground);
  const turnText = alertText("turn", ground);
  const calmAccent = alertText("calm", ground);
  const warnAccent = alertText("warning", ground);
  const secondary = light ? "rgba(0,0,0,0.55)" : "rgba(255,255,255,0.55)";
  const segmentBg = light ? "rgba(0,0,0,0.06)" : "rgba(255,255,255,0.08)";
  // PermissionAlertViews.swift line 55: "Clicking the collapsed form does nothing on purpose".
  // CompletionAlertCompactView opens the detail card; Quota / ResetCredit compact views open usage stats.
  const handleClick = request
    ? undefined
    : event?.kind === "completion" ||
        event?.kind === "sign-in-needed" ||
        event?.kind === "hooks-review-needed"
      ? onOpen
      : onDismissAndStats;

  // Each kind's glyph, drawn or filled, and the colour of its words.
  const look: {
    Icon: typeof Hand;
    color: string;
    stroke?: number;
    filled?: boolean;
    words?: string;
  } = request
    ? { Icon: Hand, color: signalText, filled: true, words: secondary }
    : event?.kind === "completion"
      ? { Icon: MessageSquareText, color: turnText, stroke: 2.2 }
      : event?.kind === "sign-in-needed"
        ? { Icon: KeyRound, color: warnAccent, stroke: 2.4 }
        : event?.kind === "hooks-review-needed"
          ? { Icon: ShieldAlert, color: warnAccent, stroke: 2.4 }
          : event?.kind === "reset-credits" || event?.kind === "quota-reset"
            ? { Icon: RotateCcw, color: calmAccent, stroke: 2.4 }
            : { Icon: Flame, color: warnAccent, filled: true };
  const trailingWhat = (
    <>
      <look.Icon
        size={glyphSize}
        strokeWidth={look.stroke}
        color={look.color}
        fill={look.filled ? look.color : "none"}
        style={{ flexShrink: 0 }}
      />
      <span
        style={{
          fontSize: font,
          fontWeight: 500,
          color: look.words ?? look.color,
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
        }}
      >
        {status}
      </span>
      {badge && (
        <span
          style={{
            fontSize: onDock ? Math.round(8 * scale) : 10,
            fontWeight: 600,
            fontVariantNumeric: "tabular-nums",
            padding: "1px 4px",
            borderRadius: 4,
            background: segmentBg,
            flexShrink: 0,
          }}
        >
          {badge}
        </span>
      )}
    </>
  );

  return (
    <div
      className="hud-compact-request"
      role={handleClick ? "button" : undefined}
      onClick={handleClick}
      style={{
        cursor: handleClick ? "pointer" : "default",
        position: "relative",
        height: "100%",
        display: "flex",
        alignItems: "center",
        padding: onDock
          ? `0 ${Math.round(10 * scale)}px`
          : `0 ${16 + surfaceFlare}px`,
        gap: onDock ? Math.round(8 * scale) : 0,
        fontSize: font,
        whiteSpace: "nowrap",
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: onDock ? Math.round(5 * scale) : 6,
          width: onDock ? undefined : 112,
          // No clipping here: the name ellipsizes itself, and the mark's
          // canvas reaches past the row for Clawd's jumps and props.
          flexShrink: 0,
        }}
      >
        <ReminderMark
          request={request}
          event={event}
          size={iconSize}
          light={light}
          botLogos={botLogos}
        />
        <span
          style={{
            fontSize: font,
            fontWeight: 500,
            // The row's own ink: the scene's may be the light HUD's while the
            // row itself is drawn dark (on the notch).
            color: alertInk(0.96, ground),
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}
        >
          {who}
        </span>
      </div>
      {!onDock && (
        <span
          style={{
            flex: 1,
            minWidth: Math.max(0, notchWidth - 48),
          }}
        />
      )}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: onDock ? "flex-start" : "flex-end",
          gap: onDock ? Math.round(4 * scale) : 5,
          width: onDock ? undefined : 112,
          overflow: "hidden",
          flexShrink: 0,
        }}
      >
        {trailingWhat}
      </div>
    </div>
  );
}
