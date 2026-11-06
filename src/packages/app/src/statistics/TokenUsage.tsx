import type { Dispatch, SetStateAction } from "react";
import type { StatisticsResponse } from "@agent-hud/protocol";
import {
  compact,
  modelColor,
  number,
  row,
  type StatsTheme,
} from "./StatsTheme";
import { TotalSummary } from "./StatsParts";
import { SectionFilter } from "./SectionFilter";
import { selectionFilter } from "./section-filter";
import { UsageChart } from "./UsageChart";

/** The Tokens page's token section: the total, the model and agent filters, and the usage chart. */
export function TokenUsage({
  data,
  loading,
  selectedModels,
  setModels,
  selectedProviders,
  setProviders,
  theme,
  light,
  chartHeight,
}: {
  data: StatisticsResponse | null;
  loading: boolean;
  selectedModels: string[] | null;
  setModels: Dispatch<SetStateAction<string[] | null>>;
  selectedProviders: string[] | null;
  setProviders: Dispatch<SetStateAction<string[] | null>>;
  theme: StatsTheme;
  light: boolean;
  /** The chart's height, points, where it fills a taller card. */
  chartHeight?: number;
}) {
  const zh = Boolean(theme.zh);
  const t = (en: string, cn: string) => (zh ? cn : en);
  const available = data?.available;
  const providerChoices = available?.providers ?? [];
  const modelChoices = available?.models ?? [];
  return (
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
      <SectionFilter
        agents={providerChoices}
        models={modelChoices.map((model) => {
          const value = data?.models.find((entry) => entry.id === model.id);
          return {
            id: model.id,
            name: model.name,
            agent: model.provider,
            color: modelColor(model.id),
            figure: value ? compact(value.selectedTokens) : "—",
          };
        })}
        state={selectionFilter(
          selectedProviders,
          setProviders,
          providerChoices.map((provider) => provider.id),
          selectedModels,
          setModels,
          modelChoices.map((model) => model.id),
        )}
        loading={loading}
        theme={theme}
        light={light}
      />
      {data ? (
        <UsageChart data={data} theme={theme} height={chartHeight} />
      ) : (
        <div
          style={{
            height: (chartHeight ?? 100) + 38,
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
  );
}
