import { useState } from "react";
import { Clock } from "lucide-react";
import type { Language } from "@agent-hud/ui/format";
import { useFormat } from "@agent-hud/ui/format-context";
import { useGround } from "../ground";
import { usePanelServices } from "../PanelContext";
import { palette, tabular } from "./theme";
import {
  dailyPaceLabel,
  grantExpiryLabel,
  grantPointsLabel,
  pointsAmount,
  pointsSpent,
  remainingPercentLabel,
} from "./points-detail";

/** TeleAgent's points detail in the side panel: the total, a green bar for what remains, and points that expire. */
export interface PointsGrant {
  name: string;
  points: number | null;
  expireDays: number | null;
}
export interface PointsGroup {
  total: number | null;
  remaining: number | null;
  percent: number | null;
  grant: PointsGrant | null;
}

const ROW =
  "20px 6px max-content minmax(48px, 1fr) max-content max-content" as const;

export function PointsDetail({
  groups,
  spent,
  vendor,
  light,
  mode = "remaining",
  onOpen,
}: {
  groups: readonly PointsGroup[];
  spent?: { perDay: number | null; days: number } | null;
  vendor?: string;
  light: boolean;
  /** Remaining share, or points already spent and the daily pace. */
  mode?: "remaining" | "used";
  onOpen?: () => void;
}) {
  const colors = palette(useGround(light));
  const { language } = useFormat();
  const services = usePanelServices();
  const [hovered, setHovered] = useState(false);
  const glow = light ? undefined : `0 0 8px ${colors.ok}`;
  const clickable = Boolean(vendor && (services?.selectProvider || onOpen));
  const open = () => {
    if (vendor) services?.selectProvider?.(vendor);
    onOpen?.();
  };
  return (
    <div style={{ display: "grid", rowGap: 10 }}>
      {groups.map((group, index) => {
        const showRemaining = group.remaining != null || group.percent != null;
        const grant = group.grant;
        const showGrant = Boolean(
          grant && (grant.points != null || grant.expireDays != null || grant.name),
        );
        return (
          <div key={index} style={{ display: "grid", rowGap: 8 }}>
            {group.total != null && (
              <div
                style={{
                  paddingLeft: 26,
                  fontSize: 12,
                  lineHeight: "16px",
                  color: colors.secondary,
                  ...tabular,
                }}
              >
                {language === "zh" ? "总额度" : "Total quota"}{" "}
                {pointsAmount(group.total, language)}
              </div>
            )}
            <div
              role={clickable ? "button" : undefined}
              tabIndex={clickable ? 0 : undefined}
              onClick={clickable ? open : undefined}
              onKeyDown={
                clickable
                  ? (event) => {
                      if (event.key === "Enter" || event.key === " ") {
                        event.preventDefault();
                        open();
                      }
                    }
                  : undefined
              }
              onMouseEnter={() => setHovered(true)}
              onMouseLeave={() => setHovered(false)}
              style={{
                display: "grid",
                gridTemplateColumns: ROW,
                rowGap: 8,
                alignItems: "center",
                cursor: clickable ? "pointer" : undefined,
                borderRadius: 6,
                background: hovered && clickable ? colors.selected : "transparent",
              }}
            >
              {showRemaining && (
                <RemainingRow
                  group={group}
                  perDay={spent?.perDay ?? null}
                  mode={mode}
                  language={language}
                  text={colors.text}
                  secondary={colors.secondary}
                  ok={colors.ok}
                  okText={colors.okText}
                  warning={colors.warning}
                  warningText={colors.warningText}
                  critical={colors.critical}
                  criticalText={colors.criticalText}
                  track={colors.track}
                  glow={glow}
                />
              )}
              {showGrant && grant && (
                <GrantRow
                  row={showRemaining ? 2 : 1}
                  grant={grant}
                  language={language}
                  text={colors.text}
                  secondary={colors.secondary}
                />
              )}
            </div>
          </div>
        );
      })}
    </div>
  );
}

function RemainingRow({
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
  const fill = !hot ? ok : percent != null && percent >= 90 ? critical : warning;
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

function GrantRow({
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
