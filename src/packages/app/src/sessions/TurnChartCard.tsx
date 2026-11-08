import { Layers, RotateCw } from "lucide-react";
import { Card } from "@agent-hud/ui/components/card";
import { Tabs, TabsList, TabsTrigger } from "@agent-hud/ui/components/tabs";
import type { SessionDetail } from "@agent-hud/protocol";
import { ContextChart } from "./ContextChart";
import { TurnBarsChart, type Measure } from "./TurnBarsChart";
import { TurnHover } from "./TurnHover";
import {
  card,
  CardTitle,
  ColorKey,
  compact,
  kindColor,
  kindLabel,
  row,
  type Entry,
  type Theme,
} from "./SessionTheme";

/** The marks the turn chart uses, named once above it. */
function ChartLegend({
  entries,
  measure,
  theme,
}: {
  entries: Entry[];
  measure: Measure;
  theme: Theme;
}) {
  const zh = Boolean(theme.zh);
  const kinds = (
    ["cacheWrite", "input", "reasoning", "output"] as const
  ).filter((kind) => entries.some((entry) => (entry.kinds?.[kind] ?? 0) > 0));
  const helped = entries.some((entry) => (entry.subagentNew ?? 0) > 0);
  const resent = entries.some((entry) => entry.recached != null);
  const compacted = entries.some((entry) => entry.compacted);
  return (
    <div
      style={{
        ...row,
        flexWrap: "wrap",
        columnGap: 14,
        rowGap: 4,
        color: theme.secondary,
        fontSize: 11,
      }}
    >
      {measure === "cost" ? (
        <ColorKey color={theme.accent}>
          {zh ? "按 API 价，含缓存读取" : "API prices, cache reads included"}
        </ColorKey>
      ) : (
        kinds.map((kind) => (
          <ColorKey key={kind} color={kindColor(theme, kind)}>
            {kindLabel(kind, zh)}
          </ColorKey>
        ))
      )}
      {helped && (
        <span style={{ ...row, gap: 4 }}>
          <Layers size={10} aria-hidden="true" />
          {measure === "cost"
            ? zh
              ? "子 agent 参与"
              : "Sub-agents helped"
            : zh
              ? "子 agent 参与，淡色为其部分"
              : "Sub-agents helped, their part faded"}
        </span>
      )}
      {resent && (
        <span style={{ ...row, gap: 4 }}>
          <RotateCw
            size={10}
            color={theme.warning}
            strokeWidth={2.6}
            aria-hidden="true"
          />
          {zh ? "缓存读取下降" : "Cache reads dropped"}
        </span>
      )}
      {compacted && (
        <span style={{ ...row, gap: 5, color: theme.compaction }}>
          <span
            aria-hidden="true"
            style={{
              display: "inline-block",
              height: 10,
              borderLeft: `1.5px dashed ${theme.compaction}`,
            }}
          />
          {zh ? "上下文压缩" : "Compacted"}
        </span>
      )}
    </div>
  );
}

/** The turn chart's card: a bar per turn in the picked measure, its marks named above it, and the context window under it. */
export function TurnChartCard({
  detail,
  byPrompt,
  measure,
  onMeasure,
  selectedEntry,
  onEntry,
  theme,
}: {
  detail: SessionDetail;
  byPrompt: boolean;
  measure: Measure;
  onMeasure: (measure: Measure) => void;
  selectedEntry: string | null;
  onEntry: (id: string | null) => void;
  theme: Theme;
}) {
  const zh = Boolean(theme.zh);
  const entries = detail.turns;
  // Cost bars only when every turn has a list price, so no turn looks free.
  const priced =
    byPrompt &&
    entries.length > 0 &&
    entries.every((entry) => entry.listCost != null);
  const shown: Measure = priced ? measure : "tokens";
  const hasContext = entries.some((entry) => entry.context != null);
  return (
    <Card style={{ ...card(theme, "12px 14px 10px"), gap: 8 }}>
      <CardTitle
        title={
          !byPrompt
            ? zh
              ? "Token 随时间变化"
              : "Tokens over time"
            : shown === "cost"
              ? zh
                ? "每轮费用"
                : "Cost per turn"
              : zh
                ? "每轮新增 Token"
                : "New tokens per turn"
        }
        right={
          priced ? (
            <Tabs
              value={measure}
              onValueChange={(value) => onMeasure(value as Measure)}
            >
              <TabsList
                style={{
                  height: 24,
                  background: theme.segment,
                  padding: 2,
                  borderRadius: 6,
                  gap: 1,
                }}
                aria-label={zh ? "度量" : "Measure"}
              >
                {(["tokens", "cost"] as const).map((value) => (
                  <TabsTrigger
                    key={value}
                    value={value}
                    style={{
                      height: 20,
                      border: 0,
                      fontSize: 11,
                      padding: "2px 10px",
                      borderRadius: 4,
                      color: theme.text,
                      background:
                        measure === value ? theme.selected : "transparent",
                      boxShadow:
                        measure === value && theme.light
                          ? "0 1px 2px rgba(0,0,0,.08)"
                          : "none",
                      transition:
                        "background-color 160ms ease, box-shadow 160ms ease",
                    }}
                  >
                    {value === "tokens" ? "Token" : zh ? "费用" : "Cost"}
                  </TabsTrigger>
                ))}
              </TabsList>
            </Tabs>
          ) : null
        }
        theme={theme}
      />
      {/* In a narrow column the hint goes under the legend. */}
      <div
        style={{
          ...row,
          justifyContent: "space-between",
          alignItems: "flex-start",
          flexWrap: "wrap",
          gap: "4px 12px",
        }}
      >
        <ChartLegend entries={entries} measure={shown} theme={theme} />
        {entries.length > 0 && (
          <span
            style={{
              color: theme.secondary,
              fontSize: 11,
              flexShrink: 0,
            }}
          >
            {zh
              ? "数字为最大的几轮 · 点击一轮看调用"
              : "Largest turns labelled · click a turn for its calls"}
          </span>
        )}
      </div>
      {entries.length > 0 ? (
        <TurnBarsChart
          entries={entries}
          measure={shown}
          selected={selectedEntry}
          onSelect={(entry) =>
            onEntry(selectedEntry === entry.id ? null : entry.id)
          }
          hover={(entry) => <TurnHover entry={entry} theme={theme} />}
          theme={theme}
        />
      ) : (
        <div
          style={{
            display: "grid",
            height: 120,
            placeItems: "center",
            color: theme.secondary,
            fontSize: 11,
          }}
        >
          {zh ? "暂无已记录的用量" : "No recorded usage"}
        </div>
      )}
      {hasContext && (
        <>
          <div
            style={{
              height: 1,
              background: theme.divider,
              margin: "2px 0",
            }}
          />
          <CardTitle
            title={zh ? "上下文窗口" : "Context window"}
            right={
              (zh ? "缓存读取 + 新增输入" : "Cache read + new input") +
              (detail.contextWindow != null && detail.contextWindow > 0
                ? (zh ? " · 上限 " : " · limit ") +
                  compact(detail.contextWindow)
                : "")
            }
            theme={theme}
          />
          <ContextChart
            entries={entries}
            window={detail.contextWindow}
            fill={detail.contextFill}
            selected={selectedEntry}
            theme={theme}
          />
        </>
      )}
    </Card>
  );
}
