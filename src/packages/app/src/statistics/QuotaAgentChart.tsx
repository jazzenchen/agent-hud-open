import { HUDArtwork as HUDLogo } from "@agent-hud/hud/HUDArtwork";
import {
  budgetText,
  outlookBasisText,
  outlookText,
  quotaPercentLabel,
  type QuotaDisplay,
} from "@agent-hud/ui/format";
import type { Quota } from "@agent-hud/protocol";
import {
  cycleSummary,
  type QuotaGroup,
} from "@agent-hud/hud/panel/quota-groups";
import { number, row, truncate, type StatsTheme } from "./StatsTheme";
import { QuotaCycles, levelFill } from "./QuotaCycles";

/** What the window reads now, from the snapshot Core worked out: its share, its budget, and its forecast with the pace it comes from. */
function nowLine(
  quota: Quota | undefined,
  display: QuotaDisplay,
  now: number,
  zh: boolean,
) {
  if (!quota || quota.usedPercent == null || quota.status === "stale")
    return null;
  const language = zh ? "zh" : "en";
  const parts = [quotaPercentLabel(quota.usedPercent, display)];
  const metrics = quota.metrics;
  if (metrics?.budget && Number.isFinite(metrics.budget.paceDelta))
    parts.push(budgetText(metrics.budget, language));
  const outlook = metrics?.outlook;
  if (outlook) {
    const exhaustsAt = outlook.exhaustsAt
      ? Date.parse(outlook.exhaustsAt)
      : NaN;
    const words = outlookText(
      {
        kind: outlook.kind,
        seconds: Number.isFinite(exhaustsAt)
          ? Math.max(0, (exhaustsAt - now) / 1000)
          : undefined,
        beforeReset: outlook.beforeReset,
      },
      outlook.projectedUsedPercent ?? null,
      language,
    );
    if (words && ["exhausted", "exhausts", "no-usage"].includes(outlook.kind)) {
      parts.push(words);
      if (outlook.basis) parts.push(outlookBasisText(outlook.basis, language));
    }
  }
  return parts.join(" · ");
}

/** The days a cycle chart's foot marks: every day in a week, the Mondays in
 * a month, in the window's own time zone. */
function ticks(from: number, until: number, days: number, timeZone: string) {
  const weekday = new Intl.DateTimeFormat("en-US", {
    weekday: "short",
    timeZone,
  });
  const clock = new Intl.DateTimeFormat("en-US", {
    hour: "numeric",
    hourCycle: "h23",
    timeZone,
  });
  const out: number[] = [];
  const start = Math.ceil(from / 3_600_000) * 3_600_000;
  for (let at = start; at <= until; at += 3_600_000) {
    if (clock.format(at) !== "0") continue;
    if (days > 7 && weekday.format(at) !== "Mon") continue;
    out.push(at);
    at += 23 * 3_600_000;
  }
  return out;
}

/** One agent's quota windows, a lane each: on the left what it reads now and
 * what its cycles in the stretch add up to; on the right a bar per cycle, the
 * lanes on one time axis so a five-hour cycle sits under its week. */
export function QuotaAgentChart({
  group,
  name,
  live,
  display,
  now,
  from,
  until,
  days,
  timeZone,
  theme,
  light,
}: {
  group: QuotaGroup;
  name: string;
  live: Map<string, Quota>;
  display: QuotaDisplay;
  now: number;
  from: number;
  until: number;
  days: number;
  timeZone: string;
  theme: StatsTheme;
  light: boolean;
}) {
  const zh = Boolean(theme.zh);
  const span = Math.max(1, until - from);
  const x = (at: number) => ((at - from) / span) * 100;
  const day = new Intl.DateTimeFormat(zh ? "zh-CN" : "en-US", {
    month: "numeric",
    day: "numeric",
    timeZone,
  });
  const reset = new Intl.DateTimeFormat(zh ? "zh-CN" : "en-US", {
    month: "numeric",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
    timeZone,
  });
  const lanes = "150px minmax(0, 1fr)";
  return (
    <div style={{ display: "grid", gap: 2 }}>
      <div style={{ ...row, fontSize: 12, marginBottom: 2 }}>
        <HUDLogo source={group.vendor} size={13} light={light} />
        <span style={{ ...truncate, fontWeight: 600 }}>{name}</span>
        {group.account && (
          <span style={{ ...truncate, color: theme.secondary, fontSize: 11 }}>
            {group.account}
          </span>
        )}
      </div>
      {group.windows.map((window) => {
        const quota = live.get(window.id);
        const summary = cycleSummary(window.cycles ?? []);
        const used = quota?.status !== "stale" ? quota?.usedPercent : null;
        const sub =
          quota?.allModels === false &&
          (window.windowDurationSeconds ?? 0) >= 86_400;
        const outlook = quota?.metrics?.outlook;
        const projected =
          outlook?.kind === "exhausts" && outlook.projectedUsedPercent != null
            ? outlook.projectedUsedPercent
            : null;
        return (
          <div
            key={window.id}
            style={{
              display: "grid",
              gridTemplateColumns: lanes,
              gap: 12,
              alignItems: "end",
              padding: "4px 0",
            }}
          >
            <div
              title={nowLine(quota, display, now, zh) ?? undefined}
              style={{
                display: "grid",
                paddingLeft: sub ? 12 : 0,
                minWidth: 0,
              }}
            >
              <span
                style={{ ...truncate, color: theme.secondary, fontSize: 11 }}
              >
                {sub ? "└ " : ""}
                {window.shortLabel ?? window.name}
              </span>
              <span
                style={{
                  ...number,
                  fontSize: 18,
                  fontWeight: 600,
                  lineHeight: 1.2,
                  color:
                    used != null && used >= 70
                      ? levelFill(used, theme)
                      : theme.text,
                }}
              >
                {used != null ? quotaPercentLabel(used, display) : "—"}
              </span>
              <span
                style={{
                  ...truncate,
                  ...number,
                  color: theme.tertiary,
                  fontSize: 10,
                }}
              >
                {quota?.resetAt
                  ? `${reset.format(Date.parse(quota.resetAt))}${zh ? " 重置" : " reset"}`
                  : "—"}
              </span>
              <span
                style={{
                  ...truncate,
                  ...number,
                  color: theme.tertiary,
                  fontSize: 10,
                }}
              >
                {zh
                  ? `${summary.count} 个周期${summary.average != null ? ` · 平均 ${Math.round(summary.average)}%` : ""}`
                  : `${summary.count} cycles${summary.average != null ? ` · avg ${Math.round(summary.average)}%` : ""}`}
                {summary.full > 0 && (
                  <span style={{ color: levelFill(100, theme) }}>
                    {zh
                      ? ` · ${summary.full} 次用满`
                      : ` · ${summary.full} used up`}
                  </span>
                )}
              </span>
            </div>
            <QuotaCycles
              window={window}
              from={from}
              until={until}
              now={now}
              projected={projected}
              theme={theme}
              timeZone={timeZone}
            />
          </div>
        );
      })}
      <div style={{ display: "grid", gridTemplateColumns: lanes, gap: 12 }}>
        <span />
        <div
          style={{
            position: "relative",
            height: 14,
            fontSize: 10,
            color: theme.tertiary,
          }}
        >
          {ticks(from, until, days, timeZone)
            .filter((at) => Math.abs(x(at) - x(now)) > 5)
            .map((at) => (
              <span
                key={at}
                style={{
                  position: "absolute",
                  left: `${x(at)}%`,
                  transform: "translateX(-50%)",
                  whiteSpace: "nowrap",
                }}
              >
                {day.format(at)}
                {days > 7 ? (zh ? " 周一" : " Mon") : ""}
              </span>
            ))}
          <span
            style={{
              position: "absolute",
              left: `${x(now)}%`,
              transform: "translateX(-50%)",
              color: theme.secondary,
            }}
          >
            {zh ? "现在" : "Now"}
          </span>
        </div>
      </div>
    </div>
  );
}
