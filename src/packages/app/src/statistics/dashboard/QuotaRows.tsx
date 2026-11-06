import { useState } from "react";
import { HUDArtwork as HUDLogo } from "@agent-hud/hud/HUDArtwork";
import { quotaPercentLabel } from "@agent-hud/ui/format";
import { useFormat } from "@agent-hud/ui/format-context";
import type {
  Quota,
  QuotaHistoryResponse,
  QuotaHistoryWindow,
} from "@agent-hud/protocol";
import {
  number,
  raisedCard,
  row,
  truncate,
  type StatsTheme,
} from "../StatsTheme";
import { QuotaDayBars, levelFill } from "./QuotaDayBars";
import { isLong, quotaDays, weekly, type QuotaDay } from "./quota-days";
import { useWidth } from "./Panel";

export interface QuotaRow {
  window: QuotaHistoryWindow;
  days: QuotaDay[];
}

/** Each window of the history as a row of days from `first`. */
export function quotaRows(
  data: QuotaHistoryResponse | null,
  first: number,
  count: number,
): QuotaRow[] {
  // A year's days fold into weeks, so each column stays wide enough to read.
  return (data?.windows ?? []).map((window) => ({
    window,
    days:
      count > 120
        ? weekly(quotaDays(window, first, count), isLong(window))
        : quotaDays(window, first, count),
  }));
}

const NOW = 40;
const GAP = 10;

/** Quota windows as thin rows of days on one axis: what each day left a
 * weekly window at (its own share darker, a tick where it reset), or the
 * highest a five-hour window reached that day. Grey while fine, amber from
 * 70% and red from 90%. Every window shows, none folded away. */
export function QuotaRows({
  rows,
  live,
  label,
  vendors,
  theme,
  light,
}: {
  rows: QuotaRow[];
  live: Map<string, Quota>;
  /** The label column's width, points. */
  label: number;
  /** Agents' names, to name each row's agent; null where the card does. */
  vendors: Map<string, string> | null;
  theme: StatsTheme;
  light: boolean;
}) {
  const zh = Boolean(theme.zh);
  const t = (en: string, cn: string) => (zh ? cn : en);
  const language = zh ? "zh-CN" : "en-US";
  const display = useFormat().quotaDisplay ?? "consumed";
  const { ref, width } = useWidth();
  const [hovered, setHovered] = useState<{ row: number; day: number } | null>(
    null,
  );
  const count = rows[0]?.days.length ?? 0;
  const plot = Math.max(0, width - label - NOW - GAP * 2);
  const slot = count ? plot / count : 0;
  const bar = Math.max(2, Math.min(24, slot * 0.6));
  const columns = `${label}px ${plot}px ${NOW}px`;
  const day = (at: number, weekday = false) =>
    new Date(at).toLocaleDateString(language, {
      month: "short",
      day: "numeric",
      ...(weekday ? { weekday: "short" } : {}),
    });
  const time = (at: number) =>
    new Date(at).toLocaleTimeString(language, {
      hour: "2-digit",
      minute: "2-digit",
      hour12: false,
    });
  const tip = (entry: QuotaRow, value: QuotaDay) => {
    const name = entry.window.shortLabel ?? entry.window.name;
    if (value.value === null) return t("Nothing read that day", "这天没有读数");
    const share = Math.round(value.value);
    const lines = isLong(entry.window)
      ? [
          t(`Used ${share}% of ${name}`, `用掉 ${name} 的 ${share}%`),
          value.level !== null &&
            t(
              `${Math.round(value.level)}% used by the day's end`,
              `当天结束时已用 ${Math.round(value.level)}%`,
            ),
        ]
      : [
          t(`Highest ${name}: ${share}%`, `${name} 当天最高 ${share}%`),
          value.ranOut > 0 &&
            t(`${value.ranOut} ran out`, `${value.ranOut} 次用满`),
        ];
    if (value.resets.length)
      lines.push(
        t(`Reset at ${time(value.resets[0])}`, `${time(value.resets[0])} 重置`),
      );
    return lines.filter(Boolean).join(" · ");
  };
  // The first and last days, and evenly between them where they fit.
  const labelEvery = Math.max(1, Math.ceil(count / Math.max(1, plot / 60)));
  const ticks = [
    ...Array.from({ length: count }, (_, column) => column).filter(
      (column) =>
        column % labelEvery === 0 && column <= count - 1 - labelEvery / 2,
    ),
    count - 1,
  ];
  if (!rows.length) return null;
  return (
    <div
      ref={ref}
      style={{ display: "grid", position: "relative", minWidth: 0 }}
      onMouseLeave={() => setHovered(null)}
    >
      {width > 0 &&
        rows.map((entry, index) => {
          const quota = live.get(entry.window.id);
          const used =
            quota?.status !== "stale" ? (quota?.usedPercent ?? null) : null;
          const long = isLong(entry.window);
          return (
            <div
              key={entry.window.id}
              title={entry.window.account ?? undefined}
              style={{
                display: "grid",
                gridTemplateColumns: columns,
                columnGap: GAP,
                alignItems: "center",
                height: 28,
              }}
            >
              <span style={{ ...row, gap: 6, minWidth: 0 }}>
                {vendors && (
                  <HUDLogo
                    source={entry.window.vendor}
                    size={13}
                    light={light}
                  />
                )}
                <span
                  style={{ ...truncate, fontSize: 11 }}
                  title={entry.window.name}
                >
                  {vendors && (
                    <span style={{ fontWeight: 500 }}>
                      {vendors.get(entry.window.vendor) ?? entry.window.vendor}
                      {" · "}
                    </span>
                  )}
                  <span style={{ color: theme.secondary }}>
                    {entry.window.shortLabel ?? entry.window.name}
                  </span>
                </span>
              </span>
              <QuotaDayBars
                days={entry.days}
                long={long}
                plot={plot}
                slot={slot}
                bar={bar}
                theme={theme}
                onHover={(column) => setHovered({ row: index, day: column })}
              />
              <span
                style={{
                  ...number,
                  fontSize: 12,
                  fontWeight: 600,
                  textAlign: "right",
                  color:
                    used !== null && used >= 70
                      ? levelFill(used, theme)
                      : theme.text,
                }}
              >
                {used !== null ? quotaPercentLabel(used, display) : "—"}
              </span>
            </div>
          );
        })}
      {width > 0 && (
        <div
          style={{
            display: "grid",
            gridTemplateColumns: columns,
            columnGap: GAP,
            fontSize: 10,
            color: theme.secondary,
          }}
        >
          <span />
          <svg width={plot} height={13} style={{ display: "block" }}>
            {ticks.map((column) => (
              <text
                key={column}
                x={
                  column === 0
                    ? 0
                    : column === count - 1
                      ? plot
                      : column * slot + slot / 2
                }
                y={10}
                textAnchor={
                  column === 0
                    ? "start"
                    : column === count - 1
                      ? "end"
                      : "middle"
                }
                fill={theme.secondary}
              >
                {day(rows[0].days[column].start)}
              </text>
            ))}
          </svg>
          <span style={{ textAlign: "right" }}>{t("now", "当前")}</span>
        </div>
      )}
      {hovered && rows[hovered.row] && (
        <div
          style={{
            ...raisedCard(theme),
            position: "absolute",
            top: hovered.row * 28 + 26,
            left: Math.min(
              label + GAP + hovered.day * slot,
              Math.max(0, width - 220),
            ),
            width: 210,
            fontSize: 11,
          }}
        >
          <strong style={{ fontSize: 12, fontWeight: 600 }}>
            {day(rows[hovered.row].days[hovered.day].start, true)}
          </strong>
          <span>
            {tip(rows[hovered.row], rows[hovered.row].days[hovered.day])}
          </span>
        </div>
      )}
    </div>
  );
}
