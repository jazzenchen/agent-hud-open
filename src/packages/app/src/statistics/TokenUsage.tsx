import { useState, type Dispatch, type SetStateAction } from "react";
import { ChevronDown, ChevronRight } from "lucide-react";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@agent-hud/ui/components/dropdown-menu";
import { HUDArtwork as HUDLogo } from "@agent-hud/hud/HUDArtwork";
import type { StatisticsResponse } from "@agent-hud/protocol";
import { number, plain, row, type StatsTheme } from "./StatsTheme";
import { TotalSummary } from "./StatsParts";
import { ModelFilter } from "./ModelFilter";
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
}: {
  data: StatisticsResponse | null;
  loading: boolean;
  selectedModels: string[] | null;
  setModels: Dispatch<SetStateAction<string[] | null>>;
  selectedProviders: string[] | null;
  setProviders: Dispatch<SetStateAction<string[] | null>>;
  theme: StatsTheme;
  light: boolean;
}) {
  const zh = Boolean(theme.zh);
  const t = (en: string, cn: string) => (zh ? cn : en);
  const [modelsExpanded, setModelsExpanded] = useState(false);
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
      <ModelFilter
        expanded={modelsExpanded}
        data={data}
        modelChoices={modelChoices}
        loading={loading}
        selectedModels={selectedModels}
        setModels={setModels}
        theme={theme}
        light={light}
      />
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
  );
}
