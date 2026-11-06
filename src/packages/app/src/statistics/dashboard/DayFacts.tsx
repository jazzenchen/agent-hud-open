import { compact, number, type StatsTheme } from "../StatsTheme";
import type { DayFacts } from "./days";

/** The heatmap's five steps, none to most, GitHub's greens: darker for
 * more on a light ground, brighter for more on a dark one. */
const GREENS = {
  light: ["rgba(0,0,0,.06)", "#c2e7cb", "#7ccb92", "#3a9d57", "#1d6b35"],
  dark: ["rgba(255,255,255,.06)", "#0e4429", "#006d32", "#26a641", "#39d353"],
};

/** A heatmap level's colour, 0 for none to 4 for most. */
export function heatColor(level: number, theme: StatsTheme) {
  return GREENS[theme.light ? "light" : "dark"][level] ?? GREENS.dark[0];
}

function Fact({
  label,
  value,
  caption,
  theme,
}: {
  label: string;
  value: string;
  caption?: string;
  theme: StatsTheme;
}) {
  return (
    <div style={{ display: "grid", gap: 1, minWidth: 0 }}>
      <span style={{ fontSize: 11, color: theme.secondary }}>{label}</span>
      <span style={{ ...number, fontSize: 15, fontWeight: 600 }}>{value}</span>
      {caption && (
        <span
          style={{
            fontSize: 11,
            color: theme.secondary,
            whiteSpace: "nowrap",
            overflow: "hidden",
            textOverflow: "ellipsis",
          }}
        >
          {caption}
        </span>
      )}
    </div>
  );
}

/** What the heatmap says in words, in a row under it. */
export function DayFactList({
  facts,
  dateText,
  theme,
}: {
  facts: DayFacts;
  dateText: (start: number) => string;
  theme: StatsTheme;
}) {
  const zh = Boolean(theme.zh);
  const t = (en: string, cn: string) => (zh ? cn : en);
  return (
    <div
      style={{
        display: "grid",
        gridTemplateColumns: "repeat(4,minmax(0,1fr))",
        gap: 12,
        paddingTop: 12,
        borderTop: `1px solid ${theme.divider}`,
      }}
    >
      <Fact
        label={t("Active days", "活跃天数")}
        value={`${facts.activeDays} / ${facts.recordedDays}`}
        caption={t("Days with any tokens", "有用量的天数")}
        theme={theme}
      />
      <Fact
        label={t("An active day", "活跃日平均")}
        value={facts.average === null ? "—" : compact(facts.average)}
        caption={t("Tokens on average", "平均 Token")}
        theme={theme}
      />
      <Fact
        label={t("Busiest day", "用得最多的一天")}
        value={facts.busiest ? compact(facts.busiest.value) : "—"}
        caption={facts.busiest ? dateText(facts.busiest.start) : undefined}
        theme={theme}
      />
      <Fact
        label={t("Streak", "连续使用")}
        value={t(`${facts.streak} days`, `${facts.streak} 天`)}
        caption={t(`Longest ${facts.longest} days`, `最长 ${facts.longest} 天`)}
        theme={theme}
      />
    </div>
  );
}
