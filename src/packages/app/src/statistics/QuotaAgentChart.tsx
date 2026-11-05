import { useState } from "react";
import { HUDArtwork as HUDLogo } from "@agent-hud/hud/HUDArtwork";
import {
  budgetText,
  outlookBasisText,
  outlookText,
  quotaPercentLabel,
  type QuotaDisplay,
} from "@agent-hud/ui/format";
import type { Quota } from "@agent-hud/protocol";
import { number, row, truncate, type StatsTheme } from "./StatsTheme";
import { QuotaChart } from "./QuotaChart";
import {
  windowColor,
  type QuotaGroup,
} from "@agent-hud/hud/panel/quota-groups";

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

/** One agent's quota windows: its name, a line per window with its colour
 * and what it reads now, and one chart of them all. Pointing at a window's
 * line brings its line forward. */
export function QuotaAgentChart({
  group,
  name,
  live,
  display,
  now,
  from,
  to,
  reveal,
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
  to: number;
  reveal: string;
  timeZone: string;
  theme: StatsTheme;
  light: boolean;
}) {
  const zh = Boolean(theme.zh);
  const [focus, setFocus] = useState<number | null>(null);
  const colors = group.windows.map((_, index) =>
    windowColor(index, theme.light),
  );
  return (
    <div style={{ display: "grid", gap: 4 }}>
      <div style={{ ...row, fontSize: 12 }}>
        <HUDLogo source={group.vendor} size={13} light={light} />
        <span style={{ ...truncate, fontWeight: 600 }}>{name}</span>
        {group.account && (
          <span style={{ ...truncate, color: theme.secondary, fontSize: 11 }}>
            {group.account}
          </span>
        )}
      </div>
      <div style={{ display: "grid", gap: 1 }}>
        {group.windows.map((window, index) => {
          const reading = nowLine(live.get(window.id), display, now, zh);
          return (
            <div
              key={window.id}
              onPointerEnter={() => setFocus(index)}
              onPointerLeave={() => setFocus(null)}
              style={{
                ...row,
                gap: 6,
                fontSize: 11,
                padding: "1px 0",
                opacity: focus == null || focus === index ? 1 : 0.55,
              }}
            >
              <span
                aria-hidden
                style={{
                  width: 8,
                  height: 3,
                  borderRadius: 2,
                  background: colors[index],
                  flexShrink: 0,
                }}
              />
              <span style={truncate} title={window.name}>
                {window.shortLabel ?? window.name}
              </span>
              {reading && (
                <span
                  title={reading}
                  style={{
                    ...truncate,
                    ...number,
                    marginLeft: "auto",
                    color: theme.secondary,
                  }}
                >
                  {reading}
                </span>
              )}
            </div>
          );
        })}
      </div>
      <QuotaChart
        windows={group.windows}
        colors={colors}
        focus={focus}
        from={from}
        to={to}
        display={display}
        theme={theme}
        reveal={reveal}
        timeZone={timeZone}
      />
    </div>
  );
}
