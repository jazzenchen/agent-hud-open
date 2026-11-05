import { useEffect, useMemo, useState, type CSSProperties } from "react";
import { Button } from "@agent-hud/ui/components/button";
import { vendorKey } from "@agent-hud/hud/model";
import type {
  NativeMethod,
  Session,
  Snapshot,
  StatisticsQuery,
  StatisticsResponse,
  StatisticsTokenKind,
} from "@agent-hud/protocol";
import { readJSON, storageKeys } from "./lib/storage-keys";
import { useCoreQuery } from "./lib/use-core-query";
import { Coverage, IndexingProgress, Heading } from "./statistics/StatsParts";
import {
  dimensionsLabel,
  freshKinds,
  row,
  themeFor,
} from "./statistics/StatsTheme";
import {
  TokenControls,
  validBucketsByRange,
  type Range,
} from "./statistics/TokenControls";
import { TokenUsage } from "./statistics/TokenUsage";
import { AgentCards } from "./statistics/AgentCards";
import { useStatisticsEnd } from "./statistics/use-statistics-end";
import { useProviderHighlight } from "./statistics/use-provider-highlight";
import { APIBillingCard } from "./statistics/APIBillingCard";
import { RecentUsage } from "./statistics/RecentUsage";
import { ActivityHeatmap } from "./statistics/ActivityHeatmap";
import { QuotaHistory } from "./statistics/QuotaHistory";
import { PlanValue } from "./statistics/PlanValue";
import { snapshotLanguage, systemTimeZone } from "@agent-hud/ui/format";
import { indexingText } from "@agent-hud/ui/text/indexing";

interface StatisticsProps {
  snapshot: Snapshot;
  /** The window's look (App's theme): with Match screen on, the look of the screen the window is on. */
  light: boolean;
  active?: boolean;
  action: (
    method: NativeMethod,
    params?: Record<string, unknown>,
  ) => Promise<unknown>;
  onSelectSession?: (session: Session) => void;
}
const durations: Record<Range, number> = {
  "5h": 5,
  "24h": 24,
  "7d": 168,
  "30d": 720,
};

/** Tokens page only. The containing native window owns the Tokens/Sessions tabs. */
export default function Statistics({
  snapshot,
  light,
  action,
  onSelectSession,
  active = true,
}: StatisticsProps) {
  const [range, setRange] = useState<Range>("24h");
  const [bucketMinutes, setBucketMinutes] =
    useState<StatisticsQuery["bucketMinutes"]>(60);
  const [selectedKinds, setKinds] = useState<StatisticsTokenKind[]>(freshKinds);
  const [selectedProviders, setProviders] = useState<string[] | null>(null);
  const [selectedModels, setModels] = useState<string[] | null>(null);
  const [hiddenAgentCards, setHiddenAgentCards] = useState<string[]>(
    () => (readJSON(storageKeys.hiddenAgentCards) as string[] | null) ?? [],
  );
  const [refresh, setRefresh] = useState(0);
  // generatedAt is a presentation/collector clock, not a ledger invalidation signal.
  const revision = snapshot.statisticsRevision ?? 0;
  const end = useStatisticsEnd(revision, refresh, active);
  const timeZone = useMemo(systemTimeZone, []);
  const zh = snapshotLanguage(snapshot) === "zh";
  const indexing = indexingText(snapshot.indexing, zh ? "zh" : "en");
  const t = (en: string, cn: string) => (zh ? cn : en);
  // The window's look (with Match screen on, its screen's), not the Appearance
  // setting alone, so the page, the toolbar and the hover cards drawn outside
  // the page share one look.
  const theme = useMemo(() => themeFor(light, zh), [light, zh]);
  const label = dimensionsLabel(selectedKinds, zh);
  const validBuckets = validBucketsByRange[range];
  const effectiveBucketMinutes: StatisticsQuery["bucketMinutes"] =
    validBuckets.includes(bucketMinutes) ? bucketMinutes : validBuckets[0];
  const query: StatisticsQuery = useMemo(
    () => ({
      from: new Date(end - durations[range] * 3_600_000).toISOString(),
      to: new Date(end).toISOString(),
      bucketMinutes: effectiveBucketMinutes,
      tokenKinds: selectedKinds,
      ...(selectedProviders === null ? {} : { providers: selectedProviders }),
      ...(selectedModels === null ? {} : { models: selectedModels }),
      timeZone,
    }),
    [
      end,
      range,
      effectiveBucketMinutes,
      selectedKinds,
      selectedProviders,
      selectedModels,
      timeZone,
    ],
  );
  // Only the latest query and revision publish into this page; no statistics
  // answer updates the Snapshot.
  const { data, error, fetching } = useCoreQuery<StatisticsResponse>(
    async (method, params) => {
      const response = (await action(method, params)) as StatisticsResponse;
      if (response.dataRevision < revision)
        throw new Error(
          "Statistics are catching up with the current ledger. Try refreshing again.",
        );
      return response;
    },
    "statistics",
    query,
    active,
    revision,
  );
  const loading = !data && !error && fetching;
  // The quota history runs to the latest reading: a window read again
  // moves its end along with the page's.
  const readings = snapshot.quotas
    .map((quota) => `${quota.id}@${quota.observedAt ?? ""}`)
    .join("|");
  const [historyEnd, setHistoryEnd] = useState(end);
  useEffect(() => {
    if (active) setHistoryEnd(Date.now());
  }, [end, readings, active]);
  const highlightedVendor = useProviderHighlight(
    active,
    Boolean(data),
    setProviders,
    setHiddenAgentCards,
  );
  const content: CSSProperties = {
    display: "flex",
    flexDirection: "column",
    gap: 12,
    minWidth: 0,
    fontFamily: '-apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif',
    fontSize: 13,
    lineHeight: 1.35,
    color: `var(--statistics-text, ${theme.text})`,
    background: `var(--statistics-ground, ${theme.ground})`,
  };
  return (
    <section
      className="statistics-native"
      style={content}
      aria-label={t("Token statistics", "Token 统计")}
      aria-busy={loading}
    >
      <TokenControls
        theme={theme}
        label={label}
        selectedKinds={selectedKinds}
        setKinds={setKinds}
        range={range}
        setRange={setRange}
        bucketMinutes={bucketMinutes}
        effectiveBucketMinutes={effectiveBucketMinutes}
        setBucketMinutes={setBucketMinutes}
      />
      {error && (
        <div
          role="alert"
          style={{
            ...row,
            justifyContent: "space-between",
            color: theme.secondary,
            fontSize: 12,
          }}
        >
          <span>
            {t("Refresh failed: ", "刷新失败：")}
            {error}
          </span>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => setRefresh((value) => value + 1)}
          >
            {t("Retry", "重试")}
          </Button>
        </div>
      )}
      {indexing ? (
        <IndexingProgress text={indexing} theme={theme} />
      ) : (
        data && <Coverage coverage={data.coverage} theme={theme} />
      )}
      <TokenUsage
        data={data}
        loading={loading}
        selectedModels={selectedModels}
        setModels={setModels}
        selectedProviders={selectedProviders}
        setProviders={setProviders}
        theme={theme}
        light={light}
      />
      <AgentCards
        data={data}
        hiddenAgentCards={hiddenAgentCards}
        setHiddenAgentCards={setHiddenAgentCards}
        selectedProviders={selectedProviders}
        selectedModels={selectedModels}
        label={label}
        theme={theme}
        light={light}
        highlightedVendor={highlightedVendor}
        billing={snapshot.billing}
        onSelectSession={onSelectSession}
      />
      {snapshot.billing.length > 0 && (
        <>
          <Heading theme={theme}>{t("API balances", "API 余额")}</Heading>
          <div
            className="statistics-billing-row"
            style={{
              display: "grid",
              gridTemplateColumns: `repeat(${Math.min(2, snapshot.billing.length)},minmax(0,1fr))`,
              gap: 12,
            }}
          >
            {snapshot.billing.map((account) => (
              <APIBillingCard
                key={account.id}
                account={account}
                theme={theme}
                highlighted={highlightedVendor === vendorKey(account.vendor)}
              />
            ))}
          </div>
        </>
      )}
      {snapshot.quotas.length > 0 && (
        <QuotaHistory
          quotas={snapshot.quotas}
          to={historyEnd}
          action={action}
          active={active}
          theme={theme}
          light={light}
          timeZone={timeZone}
        />
      )}
      {(snapshot.accounts?.length ?? 0) > 0 && (
        <PlanValue
          refresh={`${revision}|${historyEnd}|${(snapshot.accounts ?? [])
            .map((account) => `${account.id}:${account.plan ?? ""}`)
            .join(",")}`}
          action={action}
          active={active}
          theme={theme}
          light={light}
        />
      )}
      {data && (
        <>
          <RecentUsage periods={data.recent} label={label} theme={theme} />
          <ActivityHeatmap
            data={data.heatmap}
            available={data.available}
            theme={theme}
          />
        </>
      )}
    </section>
  );
}
