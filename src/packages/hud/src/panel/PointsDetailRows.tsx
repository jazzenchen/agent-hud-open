import { Clock } from "lucide-react";
import type { Language } from "@agent-hud/ui/format";
import {
  dailyPaceLabel,
  grantExpiryLabel,
  grantPointsLabel,
  pointsAmount,
  pointsSpent,
  remainingPercentLabel,
} from "./points-detail";
import type { PointsGrant, PointsGroup } from "./PointsDetail";
import { tabular } from "./theme";

export function RemainingRow({
  group,
  perDay,
  mode,
  language,
  text,
  secondary,
  ok,
  okText,
  warning,
  warningText,
  critical,
  criticalText,
  track,
  glow,
}: {
  group: PointsGroup;
  perDay: number | null;
  mode: "remaining" | "used";
  language: Language;
  text: string;
  secondary: string;
  ok: string;
  okText: string;
  warning: string;
  warningText: string;
  critical: string;
  criticalText: string;
  track: string;
  glow: string | undefined;
}) {
  const spent = pointsSpent(group.total, group.remaining);
  const percent =
    mode === "used" || group.percent == null
      ? null
      : Math.max(0, Math.min(100, group.percent));
  const amount = mode === "used" ? spent : group.remaining;
  const hot = false;
  const fill = !hot
    ? ok
    : percent != null && percent >= 90
      ? critical
      : warning;
  const figure = !hot
    ? okText
    : percent != null && percent >= 90
      ? criticalText
      : warningText;
  const label =
    mode === "used"
      ? language === "zh"
        ? "已用积分"
        : "Points used"
      : language === "zh"
        ? "剩余额度"
        : "Remaining";
  const pace =
    mode === "used"
      ? perDay == null
        ? "—"
        : dailyPaceLabel(perDay, language)
      : null;
  return (
    <>
      <span
        style={{
          gridColumn: 1,
          gridRow: 1,
          width: 8,
          height: 8,
          justifySelf: "center",
          borderRadius: "50%",
          background: fill,
          boxShadow: hot ? undefined : glow,
        }}
      />
      <span
        style={{
          gridColumn: 3,
          gridRow: 1,
          fontSize: 13,
          lineHeight: "16px",
          fontWeight: 600,
          color: text,
          whiteSpace: "nowrap",
        }}
      >
        {label}
      </span>
      {mode === "remaining" && (
        <span
          role="meter"
          aria-label={label}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={percent ?? undefined}
          style={{
            gridColumn: 4,
            gridRow: 1,
            position: "relative",
            height: 4,
            marginLeft: 12,
            background: track,
            borderRadius: 4,
          }}
        >
          <i
            style={{
              position: "absolute",
              inset: "0 auto 0 0",
              display: "block",
              width: `${percent ?? 0}%`,
              height: 4,
              borderRadius: 4,
              background: fill,
            }}
          />
        </span>
      )}
      <span
        style={{
          gridColumn: 5,
          gridRow: 1,
          marginLeft: 12,
          ...tabular,
          fontSize: 13,
          lineHeight: "16px",
          fontWeight: 600,
          color: mode === "used" ? text : figure,
          textAlign: "right",
          whiteSpace: "nowrap",
        }}
      >
        {mode === "used"
          ? amount == null
            ? "—"
            : pointsAmount(amount, language)
          : percent == null
            ? ""
            : remainingPercentLabel(percent, language)}
      </span>
      <span
        title={
          mode === "used"
            ? language === "zh"
              ? "近 7 天平均每天消耗的积分"
              : "Average points spent per day over the last 7 days"
            : undefined
        }
        style={{
          gridColumn: 6,
          gridRow: 1,
          marginLeft: 16,
          ...tabular,
          fontSize: 13,
          lineHeight: "16px",
          fontWeight: mode === "used" ? 400 : 600,
          color: mode === "used" ? secondary : text,
          textAlign: "right",
          whiteSpace: "nowrap",
        }}
      >
        {mode === "used"
          ? pace
          : amount == null
            ? ""
            : pointsAmount(amount, language)}
      </span>
    </>
  );
}

export function GrantRow({
  row,
  grant,
  language,
  text,
  secondary,
}: {
  row: number;
  grant: PointsGrant;
  language: Language;
  text: string;
  secondary: string;
}) {
  return (
    <>
      <Clock
        size={14}
        strokeWidth={1.75}
        color={secondary}
        style={{ gridColumn: 1, gridRow: row, justifySelf: "center" }}
        aria-hidden
      />
      <span
        style={{
          gridColumn: 3,
          gridRow: row,
          minWidth: 0,
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
          fontSize: 13,
          lineHeight: "16px",
          fontWeight: 500,
          color: text,
        }}
      >
        {grant.name}
      </span>
      <span
        style={{
          gridColumn: 5,
          gridRow: row,
          marginLeft: 12,
          ...tabular,
          fontSize: 13,
          lineHeight: "16px",
          fontWeight: 600,
          color: text,
          textAlign: "right",
          whiteSpace: "nowrap",
        }}
      >
        {grant.points == null ? "" : grantPointsLabel(grant.points, language)}
      </span>
      <span
        style={{
          gridColumn: 6,
          gridRow: row,
          marginLeft: 16,
          ...tabular,
          fontSize: 12,
          lineHeight: "16px",
          color: secondary,
          textAlign: "right",
          whiteSpace: "nowrap",
        }}
      >
        {grant.expireDays == null
          ? ""
          : grantExpiryLabel(grant.expireDays, language)}
      </span>
    </>
  );
}
