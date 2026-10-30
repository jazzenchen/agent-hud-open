import { memo, useEffect, useMemo, useState } from "react";
import { DollarSign, Flame, Percent } from "lucide-react";
import type { PanelBlockLayout, Quota, Snapshot } from "@agent-hud/protocol";
import { Button } from "@agent-hud/ui/components/button";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@agent-hud/ui/components/tooltip";
import { quotaMetric, quotaMetricAvailability } from "../quota-metrics";
import { useFormat } from "@agent-hud/ui/format-context";
import {
  dateFormat,
  localeDateTime,
  moneyAmount,
  quotaPercent,
  quotaPercentLabel,
  quotaDisplayLabel,
  tokensShort,
} from "@agent-hud/ui/format";
import { quotaAccountKey, quotaShortName, vendorKey } from "../model";
import { recentCost, recentUsage } from "../recent-usage";
import { FittingName } from "./FittingName";
import { AccountHeader, BlockNotice } from "./AccountHeader";
import { missingReadingNotice } from "./account-labels";
import { ResetCreditsRow } from "./ResetCreditsRow";
import { usePanelServices } from "../PanelContext";
import { useGround } from "../ground";
import { agentVendorName } from "../agent-artwork";
import { visibleBalances } from "../balance-rows";
import {
  measures,
  noButtonChrome,
  palette,
  tabular,
  type Logo,
  type Measure,
} from "./theme";
import { SourceTokenChart } from "./TokenChartSection";

export function MetricPicker({
  selected,
  choose,
  light,
  availability,
}: {
  selected: Measure;
  choose: (measure: Measure) => void;
  light: boolean;
  availability: Record<Measure, ReturnType<typeof quotaMetricAvailability>>;
}) {
  const zh = useFormat().language === "zh";
  const colors = palette(useGround(light));
  return (
    <TooltipProvider delayDuration={300}>
      <div
        className="hud-source-metric-picker"
        role="group"
        aria-label={zh ? "额度指标" : "Quota metric"}
        style={{ display: "flex", gap: 2, paddingRight: 2 }}
      >
        {measures.map(({ key, title, icon: Icon }) => {
          const status = availability[key];
          const hint = `${title} · ${status.hint}`;
          return (
            <Tooltip key={key}>
              <TooltipTrigger asChild>
                <span
                  title={hint}
                  tabIndex={status.selectable ? undefined : 0}
                  aria-label={status.selectable ? undefined : hint}
                  style={{ display: "inline-flex" }}
                >
                  <Button
                    variant="ghost"
                    size="icon-xs"
                    aria-label={title}
                    aria-pressed={selected === key}
                    disabled={!status.selectable}
                    onClick={() => choose(key)}
                    style={{
                      width: 20,
                      minWidth: 20,
                      height: 18,
                      minHeight: 18,
                      padding: 0,
                      borderRadius: 5,
                      color: selected === key ? colors.text : colors.tertiary,
                      background:
                        selected === key ? colors.selected : "transparent",
                      boxShadow: "none",
                    }}
                  >
                    <Icon
                      style={{ width: 10, height: 10 }}
                      strokeWidth={key === "burn" ? 2.6 : 2}
                      fill={key === "burn" ? "currentColor" : "none"}
                    />
                  </Button>
                </span>
              </TooltipTrigger>
              <TooltipContent
                side="top"
                style={{ maxWidth: 300, whiteSpace: "pre-line" }}
              >
                {hint}
              </TooltipContent>
            </Tooltip>
          );
        })}
      </div>
    </TooltipProvider>
  );
}

export function SourceQuotaRow({
  row,
  measure,
  now,
  light,
  showReset,
  onStats,
}: {
  row: Quota;
  measure: Measure;
  now: number;
  light: boolean;
  showReset: boolean;
  onStats?: () => void;
}) {
  const colors = palette(useGround(light));
  const services = usePanelServices();
  const [hovered, setHovered] = useState(false);
  const clickable = Boolean(services?.selectProvider || onStats);
  const activate = () => {
    services?.selectProvider?.(row.vendor);
    onStats?.();
  };
  const format = useFormat();
  const reading = quotaMetric(row, measure, now, format);
  const pct = quotaPercent(row.usedPercent, format.quotaDisplay);
  const projection =
    measure === "burn" &&
    reading.projectedPercent != null &&
    reading.projectedPercent > (row.usedPercent ?? 0)
      ? quotaPercent(reading.projectedPercent, format.quotaDisplay)
      : null;
  const solidPercent =
    format.quotaDisplay === "remaining" && projection != null
      ? projection
      : pct;
  const quotaLevel =
    row.status === "critical"
      ? colors.critical
      : row.status === "warning"
        ? colors.warning
        : colors.ok;
  const level =
    measure === "quota"
      ? quotaLevel
      : measure === "burn" && reading.runsOut
        ? colors.warning
        : colors.ok;
  const known = reading.available && row.status !== "stale";
  const color = known ? level : colors.tertiary;
  // The value is text: the level's text shade (darker on a light HUD).
  const levelText =
    measure === "quota"
      ? row.status === "critical"
        ? colors.criticalText
        : row.status === "warning"
          ? colors.warningText
          : colors.okText
      : measure === "burn" && reading.runsOut
        ? colors.warningText
        : colors.okText;
  const showsTrack = measure === "quota" || measure === "burn";
  return (
    <div
      className="hud-source-quota-row"
      title={`${row.name} · ${reading.hint}`}
      role={clickable ? "button" : undefined}
      tabIndex={clickable ? 0 : undefined}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      onClick={clickable ? activate : undefined}
      onKeyDown={
        clickable
          ? (e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                activate();
              }
            }
          : undefined
      }
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
          background: color,
          boxShadow: light || !known ? undefined : `0 0 8px ${color}`,
        }}
      />
      <FittingName
        name={row.name}
        short={quotaShortName(row)}
        style={{
          flex: "0 0 152px",
          width: 152,
          fontWeight: 600,
          color: colors.text,
        }}
      />
      {showsTrack ? (
        <span
          role="meter"
          aria-label={`${row.name} · ${quotaDisplayLabel(format.quotaDisplay, format.language)}`}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={pct ?? undefined}
          style={{
            position: "relative",
            height: 4,
            flex: "1 1 0",
            minWidth: 0,
            background: colors.track,
            borderRadius: 4,
            overflow: "hidden",
          }}
        >
          {projection != null && pct != null && (
            <i
              style={{
                position: "absolute",
                top: 0,
                left: `${Math.min(pct, projection)}%`,
                display: "block",
                height: 4,
                width: `${Math.abs(projection - pct)}%`,
                borderRadius: 4,
                background: reading.runsOut ? colors.warning : colors.secondary,
                opacity: 0.38,
              }}
            />
          )}
          <i
            style={{
              position: "absolute",
              inset: "0 auto 0 0",
              display: "block",
              height: 4,
              width: `${solidPercent ?? 0}%`,
              borderRadius: 4,
              background:
                row.status === "stale" ? colors.secondary : quotaLevel,
            }}
          />
        </span>
      ) : (
        <span aria-hidden="true" style={{ flex: "1 1 0", minWidth: 0 }} />
      )}
      <span
        style={{
          ...tabular,
          minWidth: 62,
          flexShrink: 0,
          textAlign: "right",
          fontSize: reading.available ? 13 : 12,
          fontWeight: 600,
          whiteSpace: "nowrap",
          color: known ? levelText : colors.secondary,
        }}
      >
        {reading.value}
      </span>
      {(showReset || measure !== "quota") && (
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
            fontWeight: 400,
            color:
              measure === "burn" && reading.runsOut
                ? colors.warning
                : colors.secondary,
          }}
        >
          {reading.detail}
        </span>
      )}
    </div>
  );
}

/** QuotaCell.swift: centered equal-width ring/number lines, fixed 40/46/60 pt rings. */
export function QuotaWidgets({
  rows,
  layout,
  measure,
  now,
  light,
  showReset,
  onStats,
}: {
  rows: Quota[];
  layout: PanelBlockLayout;
  measure: Measure;
  now: number;
  light: boolean;
  showReset: boolean;
  onStats?: () => void;
}) {
  const colors = palette(useGround(light)),
    services = usePanelServices(),
    format = useFormat(),
    clickable = Boolean(services?.selectProvider || onStats),
    widget = layout.widget ?? "bar";
  if (widget === "bar")
    return (
      <>
        {rows.map((row) => (
          <SourceQuotaRow
            key={row.id}
            {...{ row, measure, now, light, showReset, onStats }}
          />
        ))}
      </>
    );
  const size = { small: 40, medium: 46, large: 60 }[
      layout.ringSize ?? "medium"
    ],
    columns = layout.columns ?? 4;
  return (
    <div
      style={{
        display: "flex",
        flexWrap: "wrap",
        justifyContent: "center",
        gap: 8,
        padding: "4px 6px",
      }}
    >
      {rows.map((row) => {
        const metric = quotaMetric(row, measure, now, format),
          color =
            row.status === "stale"
              ? colors.secondary
              : row.status === "critical"
                ? colors.critical
                : row.status === "warning"
                  ? colors.warning
                  : colors.ok;
        // The ring is a fill; its figure and the number are text.
        const figure =
          row.status === "stale"
            ? colors.secondary
            : row.status === "critical"
              ? colors.criticalText
              : row.status === "warning"
                ? colors.warningText
                : colors.okText;
        const pct = quotaPercent(row.usedPercent, format.quotaDisplay) ?? 0;
        return (
          <div
            key={row.id}
            title={metric.hint}
            role={clickable ? "button" : undefined}
            tabIndex={clickable ? 0 : undefined}
            onClick={
              clickable
                ? () => {
                    services?.selectProvider?.(row.vendor);
                    onStats?.();
                  }
                : undefined
            }
            style={{
              flex: `0 0 calc((100% - ${(Math.min(columns, rows.length) - 1) * 8}px) / ${Math.min(columns, rows.length)})`,
              minWidth: 0,
              display: "flex",
              flexDirection: "column",
              alignItems: widget === "number" ? "flex-start" : "center",
              gap: 5,
              fontSize: 12,
              cursor: clickable ? "pointer" : undefined,
              borderRadius: 6,
            }}
          >
            {widget === "ring" ? (
              <div
                style={{
                  width: size,
                  maxWidth: "100%",
                  aspectRatio: 1,
                  position: "relative",
                }}
              >
                <svg
                  viewBox="0 0 60 60"
                  role="img"
                  aria-label={`${row.name} · ${quotaDisplayLabel(format.quotaDisplay, format.language)} · ${row.usedPercent == null ? "—" : quotaPercentLabel(row.usedPercent, format.quotaDisplay)}`}
                  style={{
                    width: "100%",
                    height: "100%",
                    transform: "rotate(-90deg)",
                  }}
                >
                  <circle
                    cx="30"
                    cy="30"
                    r="26"
                    fill="none"
                    stroke={colors.track}
                    strokeWidth="5"
                  />
                  <circle
                    cx="30"
                    cy="30"
                    r="26"
                    fill="none"
                    stroke={pct > 0 ? color : "transparent"}
                    strokeWidth="5"
                    strokeDasharray={`${pct * 1.6336} 163.36`}
                    strokeLinecap="round"
                  />
                </svg>
                <span
                  style={{
                    position: "absolute",
                    inset: 0,
                    display: "grid",
                    placeItems: "center",
                    fontSize: size === 60 ? 13 : 11,
                    fontWeight: 600,
                    color: figure,
                  }}
                >
                  {row.usedPercent == null
                    ? "--"
                    : quotaPercentLabel(row.usedPercent, format.quotaDisplay)}
                </span>
              </div>
            ) : null}
            <FittingName
              name={row.name}
              short={quotaShortName(row)}
              wrap
              style={{
                alignSelf: "stretch",
                fontSize: widget === "number" ? 11 : 12,
                color: widget === "number" ? colors.secondary : colors.text,
                textAlign: "center",
              }}
            />
            {widget === "number" ? (
              <strong style={{ color: figure, fontSize: 15 }}>
                {metric.value}
              </strong>
            ) : null}
            <span
              style={{
                fontSize: 11,
                color: colors.secondary,
                textAlign: "center",
                minHeight: 13,
              }}
            >
              {measure !== "quota" && widget === "ring"
                ? `${metric.value} · `
                : ""}
              {showReset || measure !== "quota" ? metric.detail : ""}
            </span>
          </div>
        );
      })}
    </div>
  );
}

export const SourceProviderBlock = memo(function SourceProviderBlock({
  vendor,
  rows,
  now,
  light,
  Logo,
  showReset,
  accountInfo,
  layout = {},
  snapshot,
  active = true,
  onStats,
}: {
  vendor: string;
  rows: Quota[];
  now: number;
  light: boolean;
  Logo: Logo;
  showReset: boolean;
  accountInfo: Snapshot["accounts"];
  layout?: PanelBlockLayout;
  snapshot: Snapshot;
  active?: boolean;
  onStats?: () => void;
}) {
  const offered = layout.measures?.length
    ? layout.measures
    : measures.map((m) => m.key);
  const [chosen, setMeasure] = useState<Measure>(offered[0]);
  const measure = offered.includes(chosen) ? chosen : offered[0];
  const format = useFormat();
  const availability = useMemo(
    () =>
      Object.fromEntries(
        measures.map(({ key }) => [
          key,
          quotaMetricAvailability(rows, key, now, format),
        ]),
      ) as Record<Measure, ReturnType<typeof quotaMetricAvailability>>,
    [rows, now, format],
  );
  const colors = palette(useGround(light));
  // Open's ReportView.accountSections: the rows split by account in row
  // order. Only a section whose rows name an account the readers reported
  // gets a header; rows of no account stand without one.
  const sections = useMemo(() => {
    const order: string[] = [];
    const byKey = new Map<string, Quota[]>();
    for (const row of rows) {
      const key = quotaAccountKey(row);
      if (!byKey.has(key)) {
        order.push(key);
        byKey.set(key, []);
      }
      byKey.get(key)!.push(row);
    }
    return order.map((key) => {
      const sectionRows = byKey.get(key)!;
      const id = sectionRows[0].accountId;
      const account = id
        ? (accountInfo?.find((profile) => profile.id === id) ?? null)
        : null;
      return {
        key,
        rows: sectionRows,
        account,
        current: account?.current !== false,
      };
    });
  }, [rows, accountInfo]);
  const missingReading = !snapshot.quotas.some((row) => row.vendor === vendor);
  // Without readings the block still says why, as a signed-out Claude Code
  // asks to be signed in again.
  const missingNotice = missingReading
    ? missingReadingNotice(snapshot.providers, vendor)
    : null;
  // Without a window, the block shows what the agent used in the last 24
  // hours instead (vendors.rs `recent_label`).
  const recent = missingReading
    ? recentUsage(snapshot, vendorKey(vendor))
    : null;
  const recentMoney = recent ? recentCost(recent, format.language) : null;
  const countdown = layout.countdown ?? showReset;
  return (
    <section
      className="hud-source-provider"
      style={{
        display: "flex",
        flexDirection: "column",
        gap: "var(--hud-block-gap, 8px)",
      }}
    >
      <div
        className="hud-source-provider-heading"
        style={{
          display: "flex",
          alignItems: "center",
          gap: 6,
          padding: "1px 0",
          minHeight: 20,
        }}
      >
        <span
          style={{
            width: 20,
            flex: "0 0 20px",
            display: "inline-flex",
            justifyContent: "center",
          }}
        >
          <Logo source={vendor} size={14} light={light} />
        </span>
        <strong
          style={{
            fontSize: 13,
            fontWeight: 700,
            lineHeight: "16px",
            color: colors.text,
          }}
        >
          {agentVendorName(vendor)}
        </strong>
        <span style={{ flex: 1 }} />
        {layout.widget !== "chart" && !missingReading && (
          <MetricPicker
            selected={measure}
            choose={setMeasure}
            light={light}
            availability={
              Object.fromEntries(
                Object.entries(availability).map(([key, value]) => [
                  key,
                  {
                    ...value,
                    selectable:
                      value.selectable && offered.includes(key as Measure),
                  },
                ]),
              ) as typeof availability
            }
          />
        )}
      </div>
      {layout.widget === "chart" ? (
        <SourceTokenChart
          snapshot={snapshot}
          focusVendor={vendor}
          light={light}
          Logo={Logo}
          layout={{ period: layout.period }}
          active={active}
        />
      ) : missingReading ? (
        <>
          {missingNotice && <BlockNotice text={missingNotice} light={light} />}
          {recent ? (
            <div
              className="hud-source-recent"
              title={
                recent.unpricedTokens > 0
                  ? format.language === "zh"
                    ? `其中 ${new Intl.NumberFormat("en").format(recent.unpricedTokens)} 个 Token 来自没有 API 价格的模型，未计入花费`
                    : `${new Intl.NumberFormat("en").format(recent.unpricedTokens)} of these tokens came from models with no API list price and are not in the cost`
                  : undefined
              }
              style={{
                display: "flex",
                justifyContent: "space-between",
                gap: 8,
                color: colors.secondary,
              }}
            >
              <span>
                {format.language === "zh" ? "最近 24 小时" : "Last 24 hours"}
              </span>
              <strong style={{ color: colors.text }}>
                {`${tokensShort(recent.tokens)} Token`}
                {recentMoney ? ` · ${recentMoney}` : ""}
              </strong>
            </div>
          ) : (
            <div
              style={{
                display: "flex",
                justifyContent: "space-between",
                color: colors.secondary,
              }}
            >
              <span>{format.language === "zh" ? "额度" : "Quota"}</span>
              <strong>--</strong>
            </div>
          )}
        </>
      ) : (
        sections.map(({ key, rows: sectionRows, account, current }) => {
          // Earned resets belong to the signed-in Codex account.
          const resets =
            account &&
            current &&
            sectionRows.some((row) => vendorKey(row.vendor) === "codex")
              ? account.resetCredits
              : null;
          return (
            <div
              key={key}
              className="hud-source-account-section"
              style={{
                display: "flex",
                flexDirection: "column",
                gap: "var(--hud-block-gap, 8px)",
              }}
            >
              {account && (
                <AccountHeader account={account} now={now} light={light} />
              )}
              <div
                style={{
                  display: "grid",
                  gap: "var(--hud-block-gap, 8px)",
                  opacity: current ? 1 : 0.55,
                }}
              >
                <QuotaWidgets
                  rows={sectionRows}
                  layout={layout}
                  measure={measure}
                  now={now}
                  light={light}
                  showReset={countdown}
                  onStats={onStats}
                />
              </div>
              {resets && (
                <ResetCreditsRow
                  resets={resets}
                  showExpiry={countdown}
                  light={light}
                />
              )}
            </div>
          );
        })
      )}
    </section>
  );
});

export function SourceBalances({
  billing,
  light,
  Logo,
  layout = {},
  onStats,
}: {
  billing: Snapshot["billing"];
  light: boolean;
  Logo: Logo;
  layout?: PanelBlockLayout;
  onStats?: () => void;
}) {
  const colors = palette(useGround(light));
  const services = usePanelServices();
  const clickable = Boolean(services?.selectProvider || onStats);
  const [period, setPeriod] = useState<"day" | "week" | "month">(
    layout.period ?? "day",
  );
  const [measure, setMeasure] = useState<"quota" | "burn" | "cost">("quota");
  useEffect(() => setPeriod(layout.period ?? "day"), [layout.period]);
  const { language } = useFormat();
  const money = (amount: number, currency: string) =>
    moneyAmount(amount, currency, language);
  return (
    <>
      {visibleBalances(billing, layout).map((account) => {
        const metrics = account.metrics?.periods[period],
          cost = metrics?.cost,
          burn = metrics?.burnRate;
        const selected =
          layout.measures?.find((value) =>
            ["quota", "burn", "cost"].includes(value),
          ) ?? measure;
        const points = metrics?.series ?? [],
          peak = Math.max(0.00001, ...points.map((point) => point.amount));
        const columns =
          selected === "burn"
            ? [
                {
                  name: language === "zh" ? "每日消耗" : "Consumption / day",
                  value: burn ? `${money(burn.perDay, burn.currency)}/d` : "—",
                },
                {
                  name: language === "zh" ? "预计用完" : "Estimated depletion",
                  value: metrics?.runsOutAt
                    ? dateFormat(language === "zh" ? "zh-CN" : "en-US").format(
                        new Date(metrics.runsOutAt),
                      )
                    : "—",
                },
              ]
            : selected === "cost"
              ? [
                  {
                    name: language === "zh" ? "估算花费" : "Estimated cost",
                    value: cost ? money(cost.amount, cost.currency) : "—",
                  },
                  {
                    name: language === "zh" ? "输出速度" : "Output speed",
                    value: metrics?.replyRate
                      ? `${metrics.replyRate.lowerBound ? "≥" : ""}${metrics.replyRate.tokensPerSecond.toFixed(1)} tok/s`
                      : "—",
                  },
                ]
              : [
                  {
                    name:
                      account.label || (language === "zh" ? "余额" : "Balance"),
                    value: money(account.amount, account.currency),
                  },
                  {
                    name: language === "zh" ? "估算花费" : "Estimated cost",
                    value: cost ? money(cost.amount, cost.currency) : "—",
                  },
                ];
        return (
          <section
            className="hud-source-balance"
            key={account.id}
            style={{ display: "grid", gap: 8 }}
          >
            <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
              <span
                style={{
                  width: 20,
                  display: "inline-flex",
                  justifyContent: "center",
                }}
              >
                <Logo source={account.vendor} size={14} light={light} />
              </span>
              <strong style={{ fontSize: 13, fontWeight: 700, flex: 1 }}>
                {agentVendorName(account.vendor)}
              </strong>
              {!layout.measures?.length && (
                <div
                  role="group"
                  aria-label={language === "zh" ? "余额指标" : "Balance metric"}
                  style={{ display: "flex", gap: 2 }}
                >
                  {(
                    [
                      {
                        key: "quota",
                        Icon: Percent,
                        title: language === "zh" ? "余额" : "Balance",
                      },
                      {
                        key: "burn",
                        Icon: Flame,
                        title:
                          language === "zh"
                            ? "每日消耗"
                            : "Consumption per day",
                      },
                      {
                        key: "cost",
                        Icon: DollarSign,
                        title:
                          language === "zh" ? "估算花费" : "Estimated cost",
                      },
                    ] as const
                  ).map(({ key, Icon, title }) => (
                    <button
                      key={key}
                      aria-pressed={selected === key}
                      title={title}
                      onClick={() => setMeasure(key)}
                      style={{
                        ...noButtonChrome,
                        color: selected === key ? colors.text : colors.tertiary,
                        background:
                          selected === key ? colors.selected : "transparent",
                        borderRadius: 4,
                        padding: 3,
                      }}
                    >
                      <Icon size={12} />
                    </button>
                  ))}
                </div>
              )}
            </div>
            {account.notice && (
              <div
                style={{
                  fontSize: 11,
                  paddingLeft: 26,
                  color: colors.warning,
                }}
              >
                {account.notice}
              </div>
            )}
            <div
              role="group"
              aria-label={language === "zh" ? "消耗时段" : "Consumption period"}
              style={{ display: "flex", gap: 2, justifyContent: "flex-end" }}
            >
              {(["day", "week", "month"] as const).map((value) => (
                <button
                  key={value}
                  aria-pressed={period === value}
                  onClick={() => setPeriod(value)}
                  style={{
                    ...noButtonChrome,
                    fontSize: 10,
                    padding: "2px 5px",
                    borderRadius: 4,
                    color: period === value ? colors.text : colors.tertiary,
                    background:
                      period === value ? colors.selected : "transparent",
                  }}
                >
                  {{ day: "24h", week: "7d", month: "30d" }[value]}
                </button>
              ))}
            </div>
            {layout.widget === "chart" ? (
              <div
                aria-label={
                  language === "zh"
                    ? "估算花费记录"
                    : "Estimated spending history"
                }
                title={metrics?.coverage.notice ?? undefined}
                style={{
                  display: "flex",
                  alignItems: "flex-end",
                  gap: 2,
                  height: 42,
                  paddingLeft: 26,
                }}
              >
                {points.length ? (
                  points.map((point, index) => (
                    <div
                      key={point.timestamp + index}
                      title={`${dateFormat(undefined, localeDateTime).format(new Date(point.timestamp))} · ${money(point.amount, point.currency)}`}
                      style={{
                        flex: 1,
                        minWidth: 1,
                        height: `${Math.max(2, (point.amount / peak) * 100)}%`,
                        background: colors.secondary,
                        borderRadius: "2px 2px 0 0",
                      }}
                    />
                  ))
                ) : (
                  <span style={{ fontSize: 11, color: colors.secondary }}>
                    —
                  </span>
                )}
              </div>
            ) : (
              <div
                role={clickable ? "button" : undefined}
                tabIndex={clickable ? 0 : undefined}
                onClick={
                  clickable
                    ? () => {
                        services?.selectProvider?.(account.vendor);
                        onStats?.();
                      }
                    : undefined
                }
                style={{
                  display: "flex",
                  gap: 24,
                  paddingLeft: 26,
                  paddingRight: 6,
                  cursor: clickable ? "pointer" : undefined,
                }}
              >
                {columns.map((column) => (
                  <div
                    key={column.name}
                    title={metrics?.coverage.notice ?? undefined}
                    style={{ flex: 1, display: "grid", gap: 3 }}
                  >
                    <span style={{ fontSize: 11, color: colors.secondary }}>
                      {column.name}
                    </span>
                    <span
                      style={{
                        ...tabular,
                        fontSize: 15,
                        fontWeight: 600,
                        color: colors.text,
                      }}
                    >
                      {column.value}
                    </span>
                  </div>
                ))}
              </div>
            )}
          </section>
        );
      })}
    </>
  );
}
