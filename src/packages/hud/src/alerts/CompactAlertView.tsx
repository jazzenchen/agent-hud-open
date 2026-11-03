import {
  Flame,
  Hand,
  KeyRound,
  MessageSquareText,
  RotateCcw,
} from "lucide-react";
import type { Approval, IslandEvent } from "@agent-hud/protocol";
import { labelWidth } from "../geometry";
import { agentVendorName } from "../model";
import { clientDisplayName, projectName } from "../IslandNews";
import { ReminderMark } from "./ReminderMark";
import { quotaAlertCopy } from "./quota-alert-copy";
import { systemCalendar } from "@agent-hud/ui/format";
import { alertText } from "../tokens";
import { useGround } from "../ground";

/** What a quota reminder says beside its agent (Open's QuotaAlertCopy.compactTitle). */
export function compactQuotaStatus(event: IslandEvent, zh = false): string {
  return quotaAlertCopy(event, {
    language: zh ? "zh" : "en",
    calendar: systemCalendar(navigator.language),
  }).compactTitle;
}

export function compactAlertLabels(
  request: Approval | undefined,
  event: IslandEvent | undefined,
  requestCount: number,
  zh: boolean,
): { who: string; status: string; badge?: string } {
  if (request) {
    const who =
      projectName(request.cwd) ??
      request.vendor ??
      clientDisplayName(request.source);
    const status =
      request.kind === "question"
        ? zh
          ? "待回答"
          : "Question"
        : request.kind === "plan"
          ? zh
            ? "计划待审"
            : "Plan"
          : zh
            ? "待批准"
            : "Waiting";
    return {
      who,
      status,
      badge: requestCount > 1 ? `×${requestCount}` : undefined,
    };
  }
  if (!event) return { who: "", status: "" };
  if (event.kind === "completion") {
    return {
      who: clientDisplayName(event.source),
      status: zh ? "有新回复" : "New reply",
    };
  }
  if (event.kind === "sign-in-needed") {
    return {
      who: event.title ?? agentVendorName(event.source),
      status: zh ? "需要重新登录" : "Sign in again",
    };
  }
  if (event.kind === "reset-credits") {
    const added = event.added ?? 1;
    return {
      who: agentVendorName(event.source),
      status: zh
        ? `+${added} 次重置`
        : `+${added} reset${added === 1 ? "" : "s"}`,
    };
  }
  return {
    who: agentVendorName(event.source),
    status: compactQuotaStatus(event, zh),
  };
}

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
    : event?.kind === "completion" || event?.kind === "sign-in-needed"
      ? onOpen
      : onDismissAndStats;

  const trailingWhat = request ? (
    <>
      <Hand
        size={glyphSize}
        color={signalText}
        fill={signalText}
        style={{ flexShrink: 0 }}
      />
      <span
        style={{
          fontSize: font,
          fontWeight: 500,
          color: secondary,
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
  ) : event?.kind === "completion" ? (
    <>
      <MessageSquareText
        size={glyphSize}
        strokeWidth={2.2}
        color={turnText}
        style={{ flexShrink: 0 }}
      />
      <span
        style={{
          fontSize: font,
          fontWeight: 500,
          color: turnText,
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
        }}
      >
        {status}
      </span>
    </>
  ) : event?.kind === "sign-in-needed" ? (
    <>
      <KeyRound
        size={glyphSize}
        strokeWidth={2.4}
        color={warnAccent}
        style={{ flexShrink: 0 }}
      />
      <span
        style={{
          fontSize: font,
          fontWeight: 500,
          color: warnAccent,
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
        }}
      >
        {status}
      </span>
    </>
  ) : event?.kind === "reset-credits" || event?.kind === "quota-reset" ? (
    <>
      <RotateCcw
        size={glyphSize}
        strokeWidth={2.4}
        color={calmAccent}
        style={{ flexShrink: 0 }}
      />
      <span
        style={{
          fontSize: font,
          fontWeight: 500,
          color: calmAccent,
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
        }}
      >
        {status}
      </span>
    </>
  ) : (
    <>
      <Flame
        size={glyphSize}
        color={warnAccent}
        fill={warnAccent}
        style={{ flexShrink: 0 }}
      />
      <span
        style={{
          fontSize: font,
          fontWeight: 500,
          color: warnAccent,
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
        }}
      >
        {status}
      </span>
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
