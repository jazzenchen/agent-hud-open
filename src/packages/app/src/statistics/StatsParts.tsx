import type { ReactNode } from "react";
import { Button } from "@agent-hud/ui/components/button";
import { listSeparator } from "@agent-hud/ui/format";
import { useFormat, useT } from "@agent-hud/ui/format-context";
import type { IndexingText } from "@agent-hud/ui/text/indexing";
import type { StatisticsCoverage, StatisticsTotal } from "@agent-hud/protocol";
import {
  compact,
  number,
  row,
  useAnimatedNumber,
  type StatsTheme,
} from "./StatsTheme";
import { costHelp, costText } from "./cost-text";

/** A refresh that failed, with its reason and a retry. */
export function RefreshFailed({
  error,
  onRetry,
  theme,
}: {
  error: string;
  onRetry: () => void;
  theme: StatsTheme;
}) {
  const t = useT();
  return (
    <div
      role="alert"
      style={{
        ...row,
        justifyContent: "space-between",
        color: theme.secondary,
        fontSize: 12,
      }}
    >
      <span>{t("Refresh failed: {{error}}", { error })}</span>
      <Button variant="ghost" size="sm" onClick={onRetry}>
        {t("Retry")}
      </Button>
    </div>
  );
}

export function Coverage({
  coverage,
  theme,
}: {
  coverage: StatisticsCoverage;
  theme: StatsTheme;
}) {
  const t = useT();
  const { language } = useFormat();
  if (coverage.state === "complete" && !coverage.notice) return null;
  const detail =
    coverage.notice ??
    (coverage.state === "unavailable"
      ? t("No ledger data is available for this selection.")
      : coverage.pendingSources
        ? t("Recorded usage is partial · {{count}} files pending.", {
            count: coverage.pendingSources,
          })
        : t("Recorded usage is partial."));
  return (
    <div
      role="status"
      style={{ color: theme.secondary, fontSize: 11, lineHeight: 1.45 }}
    >
      {detail}
      {coverage.unavailableProviders.length
        ? " " +
          t("Unavailable: {{agents}}.", {
            agents: coverage.unavailableProviders.join(listSeparator(language)),
          })
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
  const t = useT();
  const { language } = useFormat();
  return (
    <div style={{ textAlign: "right" }}>
      <div
        title={t(
          "Recorded tokens for exactly the selected range, agents, models and kinds",
        )}
        style={{ fontSize: 20, fontWeight: 600, lineHeight: 1.2, ...number }}
      >
        {coverage.state === "unavailable" ? "—" : compact(animatedTokens)}
      </div>
      <div
        title={costHelp(value.cost, language)}
        style={{ color: theme.secondary, fontSize: 11, marginTop: 2 }}
      >
        {t("At API prices {{cost}}", {
          cost: costText(value.cost, language),
        })}
      </div>
    </div>
  );
}
