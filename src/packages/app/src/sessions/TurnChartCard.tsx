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
import { useFormat, useT } from "@agent-hud/ui/format-context";

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
  const t = useT();
  const { language } = useFormat();
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
          {t("API prices, cache reads included")}
        </ColorKey>
      ) : (
        kinds.map((kind) => (
          <ColorKey key={kind} color={kindColor(theme, kind)}>
            {kindLabel(kind, language)}
          </ColorKey>
        ))
      )}
      {helped && (
        <span style={{ ...row, gap: 4 }}>
          <Layers size={10} aria-hidden="true" />
          {measure === "cost"
            ? t("Sub-agents helped")
            : t("Sub-agents helped, their part faded")}
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
          {t("Cache expired, context sent again")}
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
          {t("Compacted")}
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
  const t = useT();
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
            ? t("Tokens over time")
            : shown === "cost"
              ? t("Cost per turn")
              : t("New tokens per turn")
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
                aria-label={t("Measure")}
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
                    {value === "tokens" ? "Token" : t("Cost")}
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
            {t("Largest turns labelled · click a turn for its calls")}
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
          {t("No recorded usage")}
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
            title={t("Context window")}
            right={
              detail.contextWindow != null && detail.contextWindow > 0
                ? t("Cache read + new input · limit {{limit}}", {
                    limit: compact(detail.contextWindow),
                  })
                : t("Cache read + new input")
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
