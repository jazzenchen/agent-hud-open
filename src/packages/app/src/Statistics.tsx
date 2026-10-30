import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
} from "react";
import { Check, ChevronDown, ChevronRight } from "lucide-react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Button } from "@agent-hud/ui/components/button";
import { Tabs, TabsList, TabsTrigger } from "@agent-hud/ui/components/tabs";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@agent-hud/ui/components/dropdown-menu";
import { HUDArtwork as HUDLogo } from "@agent-hud/hud/HUDArtwork";
import { vendorKey } from "@agent-hud/hud/model";
import type {
  NativeMethod,
  Session,
  Snapshot,
  StatisticsQuery,
  StatisticsResponse,
  StatisticsTokenKind,
} from "./lib/types";
import { storageKeys } from "./lib/storage-keys";
import {
  ActivityHeatmap,
  APIBillingCard,
  Coverage,
  IndexingProgress,
  Heading,
  ProviderCard,
  RecentUsage,
  TotalSummary,
  UsageChart,
  agentRows,
  compact,
  dimensionsLabel,
  freshKinds,
  kindLabel,
  kinds,
  modelColor,
  number,
  plain,
  row,
  themeFor,
  truncate,
  type StatsTheme,
} from "./StatisticsParts";
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
type Range = "5h" | "24h" | "7d" | "30d";
const durations: Record<Range, number> = {
  "5h": 5,
  "24h": 24,
  "7d": 168,
  "30d": 720,
};
const validBucketsByRange: Record<Range, StatisticsQuery["bucketMinutes"][]> = {
  "5h": [15, 30, 60],
  "24h": [15, 30, 60],
  "7d": [60, 1440],
  "30d": [1440],
};
const hiddenAgentCardsKey = storageKeys.hiddenAgentCards;

/** A quiet ledger still advances at the source's quarter-hour boundary and on focus. */
function useStatisticsEnd(revision: number, refresh: number, active: boolean) {
  const [end, setEnd] = useState(() => Date.now());
  useEffect(() => {
    if (active) setEnd(Date.now());
  }, [revision, refresh, active]);
  useEffect(() => {
    if (!active) return;
    const quarter = 15 * 60_000;
    let timer: number;
    const advance = () => {
      const now = Date.now();
      setEnd(now);
      window.clearTimeout(timer);
      timer = window.setTimeout(advance, quarter - (now % quarter));
    };
    const now = Date.now();
    timer = window.setTimeout(advance, quarter - (now % quarter));
    window.addEventListener("focus", advance);
    return () => {
      window.clearTimeout(timer);
      window.removeEventListener("focus", advance);
    };
  }, [active]);
  return end;
}

/** Only the latest query/revision can publish into this page. No statistics result updates Snapshot. */
function useStatistics(
  query: StatisticsQuery,
  revision: number,
  action: StatisticsProps["action"],
  active: boolean,
) {
  const actionRef = useRef(action);
  actionRef.current = action;
  const sequence = useRef(0);
  const [result, setResult] = useState<{
    key: string;
    data: StatisticsResponse;
  } | null>(null);
  const [failure, setFailure] = useState<{
    key: string;
    message: string;
  } | null>(null);
  const [fetching, setFetching] = useState(active);
  const key = JSON.stringify(query);
  useEffect(() => {
    const id = ++sequence.current;
    if (!active) return;
    setFetching(true);
    const params = JSON.parse(key) as Record<string, unknown>;
    void actionRef
      .current("statistics", params)
      .then((value) => {
        if (sequence.current !== id) return;
        const response = value as StatisticsResponse;
        if (
          !response ||
          response.kind !== "statistics" ||
          !Number.isFinite(response.dataRevision)
        )
          throw new Error(
            "The runtime returned an unsupported statistics response.",
          );
        if (response.dataRevision < revision)
          throw new Error(
            "Statistics are catching up with the current ledger. Try refreshing again.",
          );
        setResult({ key, data: response });
        setFailure(null);
      })
      .catch((error: unknown) => {
        if (sequence.current === id)
          setFailure({
            key,
            message: error instanceof Error ? error.message : String(error),
          });
      })
      .finally(() => {
        if (sequence.current === id) setFetching(false);
      });
    return () => {
      sequence.current += 1;
    };
  }, [key, revision, active]);
  const data = result?.data ?? null;
  const error = failure?.key === key ? failure.message : null;
  return {
    data,
    error,
    loading: !data && !error && fetching,
    updating: Boolean(data && fetching),
    available: result?.data.available,
  };
}

function Segments({
  value,
  onChange,
  choices,
  theme,
  label,
}: {
  value: string;
  onChange: (value: string) => void;
  choices: { value: string; label: string }[];
  theme: StatsTheme;
  label: string;
}) {
  return (
    <Tabs
      value={value}
      onValueChange={onChange}
      style={{ gap: 0 }}
      aria-label={label}
    >
      <TabsList
        style={{
          background: theme.segment,
          borderRadius: 7,
          height: 28,
          padding: 2,
          gap: 1,
        }}
      >
        {choices.map((choice) => (
          <TabsTrigger
            key={choice.value}
            value={choice.value}
            style={{
              height: 24,
              padding: "3px 11px",
              border: 0,
              borderRadius: 5,
              fontSize: 12,
              fontWeight: value === choice.value ? 500 : 400,
              lineHeight: "16px",
              color: theme.text,
              boxShadow:
                value === choice.value && theme.light
                  ? "0 1px 2px rgba(0,0,0,.08)"
                  : "none",
              background:
                value === choice.value ? theme.selected : "transparent",
              transition:
                "background-color 160ms ease, color 160ms ease, box-shadow 160ms ease",
            }}
          >
            {choice.label}
          </TabsTrigger>
        ))}
      </TabsList>
    </Tabs>
  );
}

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
  const [modelsExpanded, setModelsExpanded] = useState(false);
  const [hiddenAgentCards, setHiddenAgentCards] = useState<string[]>(() => {
    try {
      const raw = localStorage.getItem(hiddenAgentCardsKey);
      const parsed = raw ? JSON.parse(raw) : [];
      return Array.isArray(parsed)
        ? parsed.filter((x) => typeof x === "string")
        : [];
    } catch {
      return [];
    }
  });
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
  const rangeChoices = useMemo(
    () => [
      { value: "5h", label: zh ? "5 小时" : "5 h" },
      { value: "24h", label: zh ? "24 小时" : "24 h" },
      { value: "7d", label: zh ? "7 天" : "7 days" },
      { value: "30d", label: zh ? "30 天" : "30 days" },
    ],
    [zh],
  );
  const bucketChoices = useMemo(() => {
    const all: { value: StatisticsQuery["bucketMinutes"]; label: string }[] = [
      { value: 15, label: zh ? "15 分钟" : "15min" },
      { value: 30, label: zh ? "30 分钟" : "30min" },
      { value: 60, label: zh ? "1 小时" : "1hr" },
      { value: 1440, label: zh ? "1 天" : "1d" },
    ];
    return all
      .filter((choice) => validBuckets.includes(choice.value))
      .map((choice) => ({ value: String(choice.value), label: choice.label }));
  }, [validBuckets, zh]);
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
  const { data, error, loading, available } = useStatistics(
    query,
    revision,
    action,
    active,
  );
  const providerChoices = available?.providers ?? [];
  const modelChoices = available?.models ?? [];
  const visibleProviders = useMemo(
    () =>
      (data?.providers ?? []).filter(
        (provider) => !hiddenAgentCards.includes(provider.id),
      ),
    [data?.providers, hiddenAgentCards],
  );
  const toggleAgentCard = (providerId: string, shown: boolean) => {
    setHiddenAgentCards((previous) => {
      const next = shown
        ? previous.filter((id) => id !== providerId)
        : [...new Set([...previous, providerId])];
      localStorage.setItem(hiddenAgentCardsKey, JSON.stringify(next));
      return next;
    });
  };
  const [highlightedVendor, setHighlightedVendor] = useState<string | null>(
    null,
  );
  const highlightTimerRef = useRef<number | null>(null);
  const applyProviderHighlight = (providerName: string) => {
    const key = vendorKey(providerName);
    if (!key) return;
    setHighlightedVendor(key);
    setProviders(null);
    setHiddenAgentCards((previous) => {
      const next = previous.filter((id) => vendorKey(id) !== key);
      if (next.length !== previous.length) {
        try {
          localStorage.setItem(hiddenAgentCardsKey, JSON.stringify(next));
        } catch {}
      }
      return next;
    });
    if (highlightTimerRef.current !== null) {
      window.clearTimeout(highlightTimerRef.current);
    }
    highlightTimerRef.current = window.setTimeout(() => {
      setHighlightedVendor(null);
      highlightTimerRef.current = null;
    }, 2500);
  };
  useEffect(() => {
    if (!active) return;
    const consumePending = () => {
      try {
        const raw = localStorage.getItem(storageKeys.highlightProvider);
        if (!raw) return;
        localStorage.removeItem(storageKeys.highlightProvider);
        const parsed = JSON.parse(raw) as { provider?: string; at?: number };
        if (
          parsed?.provider &&
          (!parsed.at || Date.now() - parsed.at < 15_000)
        ) {
          applyProviderHighlight(parsed.provider);
        }
      } catch {}
    };
    consumePending();
    window.addEventListener("focus", consumePending);
    window.addEventListener("storage", consumePending);
    const sub = isTauri()
      ? listen<{ provider: string; at: number }>(
          "statistics:provider",
          (event) => {
            try {
              localStorage.removeItem(storageKeys.highlightProvider);
            } catch {}
            if (event.payload?.provider) {
              applyProviderHighlight(event.payload.provider);
            }
          },
        )
      : null;
    return () => {
      window.removeEventListener("focus", consumePending);
      window.removeEventListener("storage", consumePending);
      if (sub) void sub.then((unlisten) => unlisten());
    };
  }, [active]);
  useEffect(() => {
    if (!highlightedVendor || !data) return;
    const frame = window.requestAnimationFrame(() => {
      const target =
        document.getElementById(`agent-${highlightedVendor}`) ??
        document.getElementById(`api-${highlightedVendor}`);
      target?.scrollIntoView({ behavior: "smooth", block: "center" });
    });
    return () => window.cancelAnimationFrame(frame);
  }, [highlightedVendor, Boolean(data)]);
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
  const pickModel = (id: string) =>
    setModels((previous) => {
      if (previous === null) return [id];
      if (previous.includes(id)) {
        const next = previous.filter((value) => value !== id);
        return next.length ? next : null;
      }
      return [...previous, id].sort();
    });
  return (
    <section
      className="statistics-native"
      style={content}
      aria-label={t("Token statistics", "Token 统计")}
      aria-busy={loading}
    >
      <div
        className="statistics-token-controls"
        style={{
          ...row,
          position: "sticky",
          top: 44,
          zIndex: 15,
          background: `var(--statistics-ground, ${theme.ground})`,
          gap: 10,
          minHeight: 28,
          paddingTop: 6,
          paddingBottom: 12,
          marginBottom: 4,
          borderBottom: `1px solid ${theme.divider}`,
          flexWrap: "wrap",
        }}
      >
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <button
              type="button"
              aria-label={t("Token kinds counted", "计入的 Token 类型")}
              style={{
                ...plain,
                ...row,
                fontSize: 12,
                fontWeight: 500,
                height: 28,
                padding: "0 10px",
                borderRadius: 6,
                background: theme.segment,
              }}
            >
              Token · {label}
              <ChevronDown size={11} style={{ opacity: 0.7 }} />
            </button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start">
            <DropdownMenuCheckboxItem
              checked={
                selectedKinds.length === freshKinds.length &&
                !selectedKinds.includes("cacheRead")
              }
              onSelect={(event) => event.preventDefault()}
              onCheckedChange={() => setKinds([...freshKinds])}
            >
              {t("Excl. cache reads", "不含缓存读取")}
            </DropdownMenuCheckboxItem>
            <DropdownMenuCheckboxItem
              checked={selectedKinds.length === kinds.length}
              onSelect={(event) => event.preventDefault()}
              onCheckedChange={() => setKinds([...kinds])}
            >
              {t("All tokens", "全部 Token")}
            </DropdownMenuCheckboxItem>
            <DropdownMenuSeparator />
            {kinds.map((kind) => (
              <DropdownMenuCheckboxItem
                key={kind}
                checked={selectedKinds.includes(kind)}
                onSelect={(event) => event.preventDefault()}
                onCheckedChange={(selected) =>
                  setKinds((previous) =>
                    selected
                      ? kinds.filter(
                          (value) => value === kind || previous.includes(value),
                        )
                      : previous.length > 1
                        ? previous.filter((value) => value !== kind)
                        : previous,
                  )
                }
              >
                {kindLabel(kind, zh)}
              </DropdownMenuCheckboxItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
        <span style={{ flex: 1, minWidth: 12 }} />
        {bucketChoices.length > 1 && (
          <Segments
            label={t("Time bucket", "时间粒度")}
            value={String(effectiveBucketMinutes)}
            onChange={(value) =>
              setBucketMinutes(
                Number(value) as StatisticsQuery["bucketMinutes"],
              )
            }
            choices={bucketChoices}
            theme={theme}
          />
        )}
        <Segments
          label={t("Statistics range", "统计范围")}
          value={range}
          onChange={(value) => {
            const nextRange = value as Range;
            setRange(nextRange);
            const allowed = validBucketsByRange[nextRange];
            if (!allowed.includes(bucketMinutes)) {
              setBucketMinutes(
                nextRange === "7d" ? 60 : allowed[allowed.length - 1],
              );
            }
          }}
          choices={rangeChoices}
          theme={theme}
        />
      </div>
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
      <div
        className="statistics-consumption"
        style={{ padding: "6px 16px 8px", minWidth: 0 }}
      >
        <div
          style={{
            ...row,
            alignItems: "baseline",
            justifyContent: "space-between",
            marginBottom: 6,
          }}
        >
          <strong style={{ fontSize: 13, fontWeight: 600 }}>
            {t("Tokens", "Token")}
          </strong>
          {data ? (
            <TotalSummary
              value={data.totals}
              coverage={data.coverage}
              theme={theme}
            />
          ) : (
            <span style={{ color: theme.secondary, ...number }}>
              {loading ? t("Loading…", "加载中…") : "—"}
            </span>
          )}
        </div>
        <div
          style={{
            ...row,
            justifyContent: "space-between",
            color: theme.secondary,
            fontSize: 11,
          }}
        >
          <div style={{ ...row, gap: 14 }}>
            <button
              type="button"
              onClick={() => setModelsExpanded((value) => !value)}
              aria-expanded={modelsExpanded}
              style={{ ...plain, ...row, gap: 4 }}
            >
              <ChevronRight
                size={10}
                style={{
                  transform: modelsExpanded ? "rotate(90deg)" : "rotate(0deg)",
                  transition: "transform 180ms cubic-bezier(0.22, 1, 0.36, 1)",
                }}
              />
              {t("Models", "模型")} ·{" "}
              {selectedModels === null
                ? t("All", "全部")
                : t(
                    `${selectedModels.length} selected`,
                    `已选 ${selectedModels.length} 个`,
                  )}
            </button>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <button
                  type="button"
                  style={{ ...plain, ...row, gap: 4 }}
                  aria-label={t("Filter statistics by agent", "按智能体筛选")}
                >
                  {t("Agents", "智能体")} ·{" "}
                  {selectedProviders === null
                    ? t("All", "全部")
                    : t(
                        `${selectedProviders.length} selected`,
                        `已选 ${selectedProviders.length} 个`,
                      )}
                  <ChevronDown size={10} />
                </button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="start">
                <DropdownMenuCheckboxItem
                  checked={selectedProviders === null}
                  onSelect={(event) => event.preventDefault()}
                  onCheckedChange={() => setProviders(null)}
                >
                  {t("All agents", "全部智能体")}
                </DropdownMenuCheckboxItem>
                <DropdownMenuSeparator />
                {providerChoices.map((provider) => (
                  <DropdownMenuCheckboxItem
                    key={provider.id}
                    checked={
                      selectedProviders === null ||
                      selectedProviders.includes(provider.id)
                    }
                    onSelect={(event) => event.preventDefault()}
                    onCheckedChange={(checked) =>
                      setProviders((previous) => {
                        const values =
                          previous ?? providerChoices.map((entry) => entry.id);
                        return checked
                          ? [...new Set([...values, provider.id])].sort()
                          : values.filter((value) => value !== provider.id);
                      })
                    }
                  >
                    <HUDLogo source={provider.id} size={12} light={light} />
                    {provider.name}
                  </DropdownMenuCheckboxItem>
                ))}
                {!providerChoices.length && (
                  <span
                    style={{ padding: 8, color: theme.secondary, fontSize: 11 }}
                  >
                    {t("No recorded agents", "暂无已记录的智能体")}
                  </span>
                )}
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
          {(selectedModels !== null || selectedProviders !== null) && (
            <button
              type="button"
              onClick={() => {
                setModels(null);
                setProviders(null);
              }}
              style={{ ...plain, color: theme.secondary, fontSize: 11 }}
            >
              {t("Clear filter", "清除筛选")}
            </button>
          )}
        </div>
        <div
          style={{
            display: "grid",
            gridTemplateRows: modelsExpanded ? "1fr" : "0fr",
            opacity: modelsExpanded ? 1 : 0,
            visibility: modelsExpanded ? "visible" : "hidden",
            pointerEvents: modelsExpanded ? "auto" : "none",
            transition:
              "grid-template-rows 200ms cubic-bezier(0.22, 1, 0.36, 1), opacity 180ms ease",
          }}
        >
          <div style={{ overflow: "hidden", minHeight: 0 }}>
            <div style={{ display: "grid", gap: 6, paddingTop: 8 }}>
              <button
                type="button"
                onClick={() => setModels(null)}
                style={{
                  ...plain,
                  ...row,
                  color: theme.secondary,
                  fontSize: 11,
                }}
              >
                <Check
                  size={11}
                  style={{ opacity: selectedModels === null ? 1 : 0 }}
                />
                {t("All models", "全部模型")}
                {selectedModels !== null &&
                  (zh ? " · 清除筛选" : " · Clear filter")}
              </button>
              <div
                style={{
                  display: "grid",
                  gridTemplateColumns: "repeat(auto-fit,minmax(190px,1fr))",
                  gap: "6px 16px",
                }}
              >
                {modelChoices.map((model) => {
                  const value = data?.models.find(
                    (entry) => entry.id === model.id,
                  );
                  const checked = selectedModels?.includes(model.id) ?? false;
                  return (
                    <button
                      key={model.id}
                      type="button"
                      role="checkbox"
                      aria-checked={checked}
                      aria-label={`Include ${model.name} from ${model.provider}`}
                      title={`${model.provider} · ${model.name}`}
                      onClick={() => pickModel(model.id)}
                      style={{
                        ...plain,
                        ...row,
                        width: "100%",
                        textAlign: "left",
                        color: checked ? theme.text : theme.secondary,
                        fontSize: 11,
                        padding: "3px 4px",
                        borderRadius: 4,
                        background: checked ? theme.segment : "transparent",
                      }}
                    >
                      <span
                        aria-hidden
                        style={{
                          width: 13,
                          height: 13,
                          flexShrink: 0,
                          borderRadius: 3.5,
                          border: `1px solid ${checked ? "#007aff" : theme.divider}`,
                          background: checked ? "#007aff" : "transparent",
                          color: "#fff",
                          display: "inline-grid",
                          placeItems: "center",
                        }}
                      >
                        {checked && <Check size={9} strokeWidth={3} />}
                      </span>
                      <span
                        aria-hidden
                        style={{
                          width: 6,
                          height: 6,
                          flexShrink: 0,
                          borderRadius: 2,
                          background: modelColor(model.id),
                        }}
                      />
                      <HUDLogo
                        source={model.provider}
                        size={12}
                        light={light}
                      />
                      <span style={truncate}>{model.name}</span>
                      <span style={{ marginLeft: "auto", ...number }}>
                        {value ? compact(value.selectedTokens) : "—"}
                      </span>
                    </button>
                  );
                })}
              </div>
              {!modelChoices.length && (
                <span style={{ color: theme.secondary, fontSize: 11 }}>
                  {loading
                    ? t("Loading models…", "正在加载模型…")
                    : t("No recorded models", "暂无已记录的模型")}
                </span>
              )}
            </div>
          </div>
        </div>
        {data ? (
          <UsageChart data={data} theme={theme} />
        ) : (
          <div
            style={{
              height: 138,
              display: "grid",
              placeItems: "center",
              color: theme.secondary,
              fontSize: 11,
            }}
          >
            {loading
              ? t("Loading token usage…", "正在加载 Token 用量…")
              : t("Token usage is unavailable", "Token 用量暂不可用")}
          </div>
        )}
      </div>
      <Heading
        theme={theme}
        trailing={
          data ? (
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <button
                  type="button"
                  style={{ ...plain, ...row, gap: 4, color: theme.secondary }}
                >
                  {t(
                    `${visibleProviders.length} shown`,
                    `显示 ${visibleProviders.length} 个`,
                  )}
                  <ChevronDown size={10} />
                </button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuCheckboxItem
                  checked={hiddenAgentCards.length === 0}
                  onSelect={(event) => event.preventDefault()}
                  onCheckedChange={() => {
                    setHiddenAgentCards([]);
                    localStorage.removeItem(hiddenAgentCardsKey);
                  }}
                >
                  {t("Show all", "全部显示")}
                </DropdownMenuCheckboxItem>
                <DropdownMenuSeparator />
                {data.providers.map((provider) => (
                  <DropdownMenuCheckboxItem
                    key={provider.id}
                    checked={!hiddenAgentCards.includes(provider.id)}
                    onSelect={(event) => event.preventDefault()}
                    onCheckedChange={(checked) =>
                      toggleAgentCard(provider.id, Boolean(checked))
                    }
                  >
                    <HUDLogo source={provider.id} size={12} light={light} />
                    {provider.name}
                  </DropdownMenuCheckboxItem>
                ))}
              </DropdownMenuContent>
            </DropdownMenu>
          ) : undefined
        }
      >
        {t("By agent", "按智能体")}
      </Heading>
      {data &&
        agentRows(visibleProviders).map((providers, index) => (
          <div
            key={index}
            className="statistics-agent-row"
            style={{
              display: "grid",
              gridTemplateColumns: `repeat(${providers.length},minmax(0,1fr))`,
              gap: 12,
            }}
          >
            {providers.map((provider) => (
              <ProviderCard
                key={provider.id}
                provider={provider}
                total={data.totals.selectedTokens}
                perRow={providers.length}
                label={label}
                theme={theme}
                highlighted={highlightedVendor === vendorKey(provider.id)}
                billing={snapshot.billing}
                onSelectSession={onSelectSession}
              />
            ))}
          </div>
        ))}
      {data && !visibleProviders.length && (
        <p style={{ color: theme.secondary, fontSize: 12, margin: "4px 0" }}>
          {selectedProviders?.length === 0 ||
          selectedModels?.length === 0 ||
          (data.providers.length > 0 && visibleProviders.length === 0)
            ? t("No agents or models selected.", "未选择智能体或模型。")
            : t(
                "No recorded agent usage in this range.",
                "该范围内无已记录的智能体用量。",
              )}
        </p>
      )}
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
