import { useState } from "react";
import type { QuotaPool, QuotaPoolMember } from "@agent-hud/protocol";
import {
  quotaDisplayLabel,
  quotaPercent,
  quotaPercentLabel,
  resetLabel,
  translate,
  type FormatSettings,
} from "@agent-hud/ui/format";
import { useFormat, useT } from "@agent-hud/ui/format-context";
import { usePanelServices } from "../PanelContext";
import { useGround } from "../ground";
import {
  hexAlpha,
  levelFill,
  levelText,
  type Ground,
  type Level,
} from "../tokens";
import { FittingName } from "./FittingName";
import { palette, tabular } from "./theme";

function statusLevel(status: QuotaPool["status"]): Level {
  return status === "critical" || status === "warning" ? status : "ok";
}

function poolResetDetail(
  pool: QuotaPool,
  now: number,
  format: FormatSettings,
): string {
  if (!pool.nextResetAt) return "—";
  const resetMs = Date.parse(pool.nextResetAt);
  if (!Number.isFinite(resetMs)) return "—";
  if (resetMs <= now) return translate(format.language, "Pending update");
  const base =
    resetMs - now < 60_000
      ? "<1m"
      : resetLabel(resetMs, now, format.calendar, format.language);
  const gain = pool.nextResetGainPercent;
  if (gain != null && gain >= 0.5) {
    return `${base} (+${Math.round(gain)}%)`;
  }
  return base;
}

function memberHint(
  member: QuotaPoolMember,
  now: number,
  format: FormatSettings,
): string {
  const pct = quotaPercentLabel(member.usedPercent, format.quotaDisplay);
  const resetMs = member.resetAt ? Date.parse(member.resetAt) : NaN;
  const reset = Number.isFinite(resetMs)
    ? resetMs <= now
      ? translate(format.language, "Pending update")
      : resetLabel(resetMs, now, format.calendar, format.language)
    : "—";
  const burn =
    member.burnRatePctPerHour != null
      ? ` · ${member.burnRatePctPerHour.toFixed(1)}%/h`
      : "";
  const plan = member.planLabel ? ` (${member.planLabel})` : "";
  return `${member.accountLabel}${plan}: ${pct}${burn} · ${reset}`;
}

function PoolRow({
  pool,
  now,
  light,
  showReset,
  onStats,
}: {
  pool: QuotaPool;
  now: number;
  light: boolean;
  showReset: boolean;
  onStats?: () => void;
}) {
  const ground: Ground = useGround(light);
  const colors = palette(ground);
  const services = usePanelServices();
  const format = useFormat();
  const [hovered, setHovered] = useState(false);
  const clickable = Boolean(services?.selectProvider || onStats);
  const activate = () => {
    services?.selectProvider?.(pool.vendor);
    onStats?.();
  };

  const pct = quotaPercent(pool.usedPercent, format.quotaDisplay) ?? 0;
  const label = quotaPercentLabel(pool.usedPercent, format.quotaDisplay);
  const level = statusLevel(pool.status);
  const known = pool.status !== "stale";
  const dotColor = known ? levelFill(level, ground) : colors.tertiary;
  const valueColor = known ? levelText(level, ground) : colors.secondary;
  const counted = pool.members.filter(
    (member) => !known || member.status !== "stale",
  );
  const count = Math.max(1, counted.length);

  const segments = counted.map((member) => {
    const share =
      format.quotaDisplay === "remaining"
        ? Math.max(0, Math.min(100, 100 - member.usedPercent)) / count
        : Math.max(0, Math.min(100, member.usedPercent)) / count;
    const mLevel = statusLevel(member.status);
    const fill =
      member.status === "stale" ? colors.secondary : levelFill(mLevel, ground);
    return {
      member,
      width: share,
      fill,
      hint: memberHint(member, now, format),
    };
  });

  const totalSolid = Math.min(
    100,
    segments.reduce((acc, item) => acc + item.width, 0),
  );
  const gainWidth =
    pool.nextResetGainPercent != null && pool.nextResetGainPercent > 0
      ? Math.min(
          100 - (format.quotaDisplay === "remaining" ? totalSolid : 0),
          pool.nextResetGainPercent,
        )
      : 0;
  const stripeTone = hexAlpha(levelFill("ok", ground), 0.55);
  const rowTooltip = [
    `${pool.name}: ${label} · ${quotaDisplayLabel(format.quotaDisplay, format.language)}`,
    ...pool.members.map((member) => memberHint(member, now, format)),
  ].join("\n");

  return (
    <div
      className="hud-source-quota-row hud-quota-pool-row"
      title={rowTooltip}
      role={clickable ? "button" : undefined}
      tabIndex={clickable ? 0 : undefined}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      onClick={clickable ? activate : undefined}
      onKeyDown={(event) => {
        if (clickable && (event.key === "Enter" || event.key === " ")) {
          event.preventDefault();
          activate();
        }
      }}
      style={{
        display: "flex",
        alignItems: "center",
        gap: 12,
        padding: "var(--hud-row-padding, 4px) 6px",
        fontSize: 13,
        lineHeight: "16px",
        minHeight: "calc(16px + var(--hud-row-padding, 4px) * 2)",
        borderRadius: 6,
        cursor: clickable ? "pointer" : undefined,
        background: hovered && clickable ? colors.selected : "transparent",
        transition: "background 0.12s ease",
      }}
    >
      <span
        style={{
          display: "block",
          width: 8,
          height: 8,
          flex: "0 0 8px",
          borderRadius: "50%",
          background: dotColor,
          boxShadow: light || !known ? undefined : `0 0 8px ${dotColor}`,
        }}
      />
      <FittingName
        name={pool.name}
        short={pool.shortLabel ?? pool.name}
        style={{
          flex: "0 0 152px",
          width: 152,
          fontWeight: 600,
          color: colors.text,
        }}
      />
      <span
        role="meter"
        aria-label={`${pool.name} · ${quotaDisplayLabel(format.quotaDisplay, format.language)}`}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={pct}
        style={{
          position: "relative",
          display: "flex",
          alignItems: "stretch",
          gap: 1,
          height: 6,
          flex: "1 1 0",
          minWidth: 0,
          background: colors.track,
          borderRadius: 4,
          overflow: "hidden",
        }}
      >
        {segments.map(({ member, width, fill, hint }) =>
          width > 0.1 ? (
            <i
              key={member.accountId}
              title={hint}
              style={{
                display: "block",
                height: "100%",
                width: `${width}%`,
                background: fill,
                flexShrink: 0,
              }}
            />
          ) : null,
        )}
        {format.quotaDisplay === "remaining" && gainWidth > 0.2 && (
          <i
            aria-hidden
            style={{
              display: "block",
              height: "100%",
              width: `${gainWidth}%`,
              flexShrink: 0,
              backgroundImage: `repeating-linear-gradient(-45deg, ${stripeTone}, ${stripeTone} 2px, transparent 2px, transparent 4px)`,
            }}
          />
        )}
      </span>
      <span
        style={{
          ...tabular,
          minWidth: 62,
          flexShrink: 0,
          textAlign: "right",
          fontSize: 13,
          fontWeight: 600,
          whiteSpace: "nowrap",
          color: valueColor,
        }}
      >
        {label}
      </span>
      {showReset && (
        <span
          style={{
            ...tabular,
            flex: "0 0 96px",
            width: 96,
            textAlign: "right",
            whiteSpace: "nowrap",
            overflow: "hidden",
            textOverflow: "ellipsis",
            fontSize: 12,
            color: colors.secondary,
          }}
        >
          {poolResetDetail(pool, now, format)}
        </span>
      )}
    </div>
  );
}

export function QuotaPoolCard({
  pools,
  now,
  light,
  showReset,
  onStats,
}: {
  pools: QuotaPool[];
  now: number;
  light: boolean;
  showReset: boolean;
  onStats?: () => void;
}) {
  const t = useT();
  const colors = palette(useGround(light));
  const recommended = pools[0]?.members.find((m) => m.recommended);

  return (
    <div
      className="hud-quota-pool-card"
      style={{
        display: "flex",
        flexDirection: "column",
        gap: "var(--hud-block-gap, 8px)",
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          gap: 8,
          padding: "2px 6px 0",
          fontSize: 11,
          lineHeight: "13px",
          color: colors.secondary,
        }}
      >
        <span>
          {t("Pooled across {{count}} accounts", {
            count: pools[0]?.members.length ?? 0,
          })}
        </span>
        {recommended && (
          <span style={{ color: colors.okText, fontWeight: 500 }}>
            {t("Best next: {{account}}", {
              account: recommended.accountLabel,
            })}
          </span>
        )}
      </div>
      {pools.map((pool) => (
        <PoolRow
          key={pool.id}
          pool={pool}
          now={now}
          light={light}
          showReset={showReset}
          onStats={onStats}
        />
      ))}
    </div>
  );
}
