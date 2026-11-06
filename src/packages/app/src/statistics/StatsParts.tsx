import type { ReactNode } from "react";
import type { IndexingText } from "@agent-hud/ui/text/indexing";
import type { StatisticsCoverage, StatisticsTotal } from "@agent-hud/protocol";
import {
  compact,
  costHelp,
  costText,
  integer,
  number,
  row,
  useAnimatedNumber,
  type StatsTheme,
} from "./StatsTheme";

export function Coverage({
  coverage,
  theme,
}: {
  coverage: StatisticsCoverage;
  theme: StatsTheme;
}) {
  if (coverage.state === "complete" && !coverage.notice) return null;
  const zh = Boolean(theme.zh);
  const detail =
    coverage.notice ??
    (coverage.state === "unavailable"
      ? zh
        ? "当前筛选暂无账本数据。"
        : "No ledger data is available for this selection."
      : zh
        ? `已记录用量尚不完整${coverage.pendingSources ? ` · ${integer(coverage.pendingSources)} 个文件待解析` : ""}。`
        : `Recorded usage is partial${coverage.pendingSources ? ` · ${integer(coverage.pendingSources)} files pending` : ""}.`);
  return (
    <div
      role="status"
      style={{ color: theme.secondary, fontSize: 11, lineHeight: 1.45 }}
    >
      {detail}
      {coverage.unavailableProviders.length
        ? zh
          ? ` 暂不可用：${coverage.unavailableProviders.join("、")}。`
          : ` Unavailable: ${coverage.unavailableProviders.join(", ")}.`
        : ""}
    </div>
  );
}

/** While the usage ledger is read through, in place of the coverage note:
 * how far back it has read, a bar of the days read, how far back the logs
 * go and that totals are incomplete meanwhile. */
export function IndexingProgress({
  text,
  theme,
}: {
  text: IndexingText;
  theme: StatsTheme;
}) {
  return (
    <div
      role="status"
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 6,
        padding: "0 16px",
      }}
    >
      <span
        style={{
          display: "flex",
          alignItems: "baseline",
          justifyContent: "space-between",
          flexWrap: "wrap",
          columnGap: 12,
        }}
      >
        <span style={{ fontSize: 13, fontWeight: 600 }}>{text.title}</span>
        {text.days ? (
          <span
            style={{
              color: theme.secondary,
              fontSize: 11,
              fontVariantNumeric: "tabular-nums",
            }}
          >
            {text.days}
          </span>
        ) : null}
      </span>
      <span
        role="progressbar"
        aria-label={text.title}
        aria-valuetext={text.days || undefined}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.floor(text.fraction * 100)}
        style={{
          position: "relative",
          height: 5,
          background: theme.track,
          borderRadius: 3,
          overflow: "hidden",
        }}
      >
        <i
          style={{
            position: "absolute",
            inset: "0 auto 0 0",
            display: "block",
            width: `${text.fraction * 100}%`,
            borderRadius: 3,
            background: theme.light ? "#007aff" : "#0a84ff",
            transition: "width 600ms ease-out",
          }}
        />
      </span>
      <span style={{ color: theme.secondary, fontSize: 11 }}>
        {text.detail}
      </span>
    </div>
  );
}

export function Heading({
  children,
  trailing,
  theme,
}: {
  children: ReactNode;
  trailing?: ReactNode;
  theme: StatsTheme;
}) {
  return (
    <div style={{ ...row, justifyContent: "space-between" }}>
      <strong style={{ fontSize: 13, fontWeight: 600 }}>{children}</strong>
      {trailing && (
        <span style={{ color: theme.secondary, fontSize: 11 }}>{trailing}</span>
      )}
    </div>
  );
}

export function Swatch({ color }: { color: string }) {
  return (
    <span
      aria-hidden
      style={{
        display: "inline-block",
        width: 8,
        height: 8,
        borderRadius: 2,
        background: color,
        flexShrink: 0,
      }}
    />
  );
}

export function TotalSummary({
  value,
  coverage,
  theme,
}: {
  value: StatisticsTotal;
  coverage: StatisticsCoverage;
  theme: StatsTheme;
}) {
  const animatedTokens = useAnimatedNumber(value.selectedTokens);
  const zh = Boolean(theme.zh);
  return (
    <div style={{ textAlign: "right" }}>
      <div
        title={
          zh
            ? "所选时间范围、智能体、模型和类型的已记录 Token 数"
            : "Recorded tokens for exactly the selected range, agents, models and kinds"
        }
        style={{ fontSize: 20, fontWeight: 600, lineHeight: 1.2, ...number }}
      >
        {coverage.state === "unavailable" ? "—" : compact(animatedTokens)}
      </div>
      <div
        title={costHelp(value.cost, zh)}
        style={{ color: theme.secondary, fontSize: 11, marginTop: 2 }}
      >
        {zh
          ? `按 API 价格估算 ${costText(value.cost, zh)}`
          : `At API prices ${costText(value.cost, zh)}`}
      </div>
    </div>
  );
}
