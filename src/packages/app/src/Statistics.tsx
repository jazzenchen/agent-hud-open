import { useMemo, useState } from "react";
import { vendorKey } from "@agent-hud/hud/model";
import type {
  NativeMethod,
  Session,
  Snapshot,
  StatisticsTokenKind,
} from "@agent-hud/protocol";
import { readJSON, storageKeys } from "./lib/storage-keys";
import {
  Coverage,
  IndexingProgress,
  RefreshFailed,
} from "./statistics/StatsParts";
import { dimensionsLabel, freshKinds, themeFor } from "./statistics/StatsTheme";
import { useStatisticsEnd } from "./statistics/use-statistics-end";
import { useProviderHighlight } from "./statistics/use-provider-highlight";
import { quotaRows } from "./statistics/dashboard/QuotaRows";
import { TokenUsage } from "./statistics/TokenUsage";
import { AgentCards } from "./statistics/AgentCards";
import { FilterBar } from "./statistics/dashboard/FilterBar";
import { Figures, spanDays } from "./statistics/dashboard/Figures";
import { ActivityCard } from "./statistics/dashboard/ActivityCard";
import { AgentBreakdown } from "./statistics/dashboard/AgentBreakdown";
import { PlanList } from "./statistics/dashboard/PlanList";
import { rangeText } from "./statistics/dashboard/range-text";
import { usePendingLook } from "./statistics/dashboard/pending";
import { pageStyle } from "./statistics/dashboard/Panel";
import {
  useOverview,
  useQuotaSpan,
  useSavedRange,
} from "./statistics/dashboard/use-overview";
import {
  planPrices,
  planReturn,
  usd,
  rangeSpan,
} from "./statistics/dashboard/model";
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

/** The Usage page: filters on top that every section follows, the big
 * figures, every day as a heatmap, the token chart beside each agent's
 * share, the plans and balances, and a card per agent with its quota by
 * day. Its grid re-flows with the window's width. The native window owns
 * the tabs. */
export default function Statistics({
  snapshot,
  light,
  action,
  active = true,
}: StatisticsProps) {
  const [range, setRange] = useSavedRange();
  const [providers, setProviders] = useState<string[] | null>(null);
  const [models, setModels] = useState<string[] | null>(null);
  const [selectedKinds, setKinds] = useState<StatisticsTokenKind[]>(freshKinds);
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
  const theme = useMemo(() => themeFor(light, zh), [light, zh]);
  const kindsLabel = dimensionsLabel(selectedKinds, zh);
  const span = useMemo(() => rangeSpan(range, end), [range, end]);
  const quotaDays = useQuotaSpan(range, end, snapshot.quotas, active);
  const overview = useOverview({
    action,
    revision,
    active,
    span,
    end,
    kinds: selectedKinds,
    providers,
    models,
    timeZone,
    accounts: (snapshot.accounts ?? [])
      .map((account) => `${account.id}:${account.plan ?? ""}`)
      .join(","),
    quotaSpan: quotaDays.span,
    reading: Boolean(snapshot.indexing && !snapshot.indexing.complete),
    rangeKey: range.id === "day" ? range.day : range.id,
  });
  const { data, year, error, loading, plans, quota, pending } = overview;
  const trendLook = usePendingLook(pending.range);
  const live = useMemo(
    () => new Map(snapshot.quotas.map((entry) => [entry.id, entry])),
    [snapshot.quotas],
  );
  const rows = useMemo(
    () =>
      quotaRows(quota, quotaDays.first, quotaDays.count).filter(
        (entry) =>
          !providers ||
          providers.some(
            (id) => vendorKey(id) === vendorKey(entry.window.vendor),
          ),
      ),
    [quota, quotaDays.first, quotaDays.count, providers],
  );
  const agents = useMemo(
    () =>
      (year?.providers ?? [])
        .filter((provider) => provider.selectedTokens > 0)
        .map((provider) => ({ id: provider.id, name: provider.name })),
    [year],
  );
  const names = useMemo(
    () =>
      new Map(
        [...(quota?.agents ?? []), ...agents].map((agent) => [
          agent.id,
          agent.name,
        ]),
      ),
    [quota, agents],
  );
  const prices = useMemo(() => planPrices(plans), [plans]);
  // The stretch the shown answer covers: while new filters wait, the old
  // figures keep their own days rather than the newly picked range's.
  const shownSpan = useMemo(
    () =>
      data
        ? { from: Date.parse(data.query.from), to: Date.parse(data.query.to) }
        : span,
    [data, span],
  );
  const returned = data ? planReturn(data, prices, shownSpan) : null;
  const highlightedVendor = useProviderHighlight(
    active,
    Boolean(data),
    setProviders,
    setHiddenAgentCards,
  );
  const { compared, words: rangeWords } = rangeText(range, span.from, zh);
  const single = providers?.length === 1 ? providers[0] : null;
  const billing = snapshot.billing.filter(
    (account) =>
      !providers ||
      providers.some((id) => vendorKey(id) === vendorKey(account.vendor)),
  );
  return (
    <section
      className="statistics-native stats-dashboard"
      style={pageStyle(theme, light)}
      aria-label={t("Usage", "用量")}
      aria-busy={loading}
    >
      <FilterBar
        agents={agents}
        selected={providers}
        setSelected={setProviders}
        selectedKinds={selectedKinds}
        setKinds={setKinds}
        kindsLabel={kindsLabel}
        range={range}
        setRange={setRange}
        theme={theme}
        light={light}
      />
      {error && (
        <RefreshFailed
          error={error}
          onRetry={() => setRefresh((value) => value + 1)}
          theme={theme}
        />
      )}
      {indexing ? (
        <IndexingProgress text={indexing} theme={theme} />
      ) : (
        data?.coverage.state === "unavailable" && (
          <Coverage coverage={data.coverage} theme={theme} />
        )
      )}
      <div className="stats-grid">
        <div className="stats-area-figures">
          <Figures
            data={data}
            prices={prices}
            plan={returned}
            compared={compared}
            days={spanDays(shownSpan)}
            agentHasPlan={!providers || providers.some((id) => prices.has(id))}
            pending={pending.range}
            theme={theme}
          />
        </div>
        <ActivityCard
          year={year}
          range={data}
          agents={providers}
          selectedDay={range.id === "day" ? range.day : null}
          onSelectDay={(day) => setRange({ id: "day", day })}
          now={end}
          focus={range.id === "1y" ? null : span}
          readBackTo={
            snapshot.indexing?.readBackTo
              ? Date.parse(snapshot.indexing.readBackTo)
              : null
          }
          pending={pending}
          theme={theme}
        />
        <div
          className={`stats-panel stats-area-trend stats-card ${trendLook.className}`}
          style={trendLook.style}
        >
          <TokenUsage
            data={data}
            loading={loading}
            selectedModels={models}
            setModels={setModels}
            selectedProviders={providers}
            setProviders={setProviders}
            theme={theme}
            light={light}
            chartHeight={196}
          />
        </div>
        <AgentBreakdown
          data={data}
          agent={single}
          pending={pending.range}
          theme={theme}
          light={light}
        />
        <PlanList
          data={plans}
          agents={providers}
          balances={billing}
          names={names}
          range={
            data && {
              value: new Map(
                data.providers.map((provider) => [
                  provider.id,
                  usd(provider.cost?.amounts) ?? 0,
                ]),
              ),
              days: spanDays(shownSpan),
              words: rangeWords,
            }
          }
          highlighted={highlightedVendor}
          pending={pending.range}
          theme={theme}
          light={light}
        />
        <div className="stats-area-cards">
          <AgentCards
            data={data}
            hiddenAgentCards={hiddenAgentCards}
            setHiddenAgentCards={setHiddenAgentCards}
            selectedProviders={providers}
            selectedModels={models}
            label={kindsLabel}
            theme={theme}
            light={light}
            highlightedVendor={highlightedVendor}
            billing={snapshot.billing}
            quota={{ rows, live }}
            names={names}
            pending={pending}
          />
        </div>
      </div>
    </section>
  );
}
