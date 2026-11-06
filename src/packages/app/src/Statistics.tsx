import { useMemo, useState, type CSSProperties } from "react";
import { Button } from "@agent-hud/ui/components/button";
import { vendorKey } from "@agent-hud/hud/model";
import type {
  NativeMethod,
  Session,
  Snapshot,
  StatisticsTokenKind,
} from "@agent-hud/protocol";
import { readJSON, storageKeys } from "./lib/storage-keys";
import { Coverage, IndexingProgress } from "./statistics/StatsParts";
import {
  dimensionsLabel,
  freshKinds,
  row,
  themeFor,
} from "./statistics/StatsTheme";
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
 * share, a card per agent, the plans and balances, their quota cycles and
 * the week's hours. Its grid re-flows with the window's width. The native
 * window owns the tabs. */
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
  });
  const { data, year, error, loading, plans, quota } = overview;
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
  const returned = data ? planReturn(data, prices, span) : null;
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
  const content: CSSProperties = {
    display: "flex",
    flexDirection: "column",
    gap: 14,
    minWidth: 0,
    fontFamily: '-apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif',
    fontSize: 13,
    lineHeight: 1.35,
    color: `var(--statistics-text, ${theme.text})`,
    background: `var(--statistics-ground, ${theme.ground})`,
    // The frame of the sections drawn without a Panel of their own.
    ["--stats-border" as string]: theme.border,
    ["--stats-card" as string]: `var(--statistics-card, ${theme.card})`,
  };
  return (
    <section
      className="statistics-native stats-dashboard"
      style={content}
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
            days={spanDays(span)}
            agentHasPlan={!providers || providers.some((id) => prices.has(id))}
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
          theme={theme}
        />
        <div className="stats-panel stats-area-trend stats-card">
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
          />
        </div>
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
              days: spanDays(span),
              words: rangeWords,
            }
          }
          highlighted={highlightedVendor}
          theme={theme}
          light={light}
        />
      </div>
    </section>
  );
}
