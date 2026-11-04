import { useState } from "react";
import type { StatisticsModel } from "@agent-hud/protocol";
import { share } from "@agent-hud/ui/format";
import {
  compact,
  costText,
  integer,
  modelColor,
  number,
  plain,
  row,
  truncate,
  type StatsTheme,
} from "./StatsTheme";
import { Swatch } from "./StatsParts";

export function ModelLine({
  model,
  total,
  theme,
}: {
  model: StatisticsModel;
  total: number;
  theme: StatsTheme;
}) {
  const zh = Boolean(theme.zh);
  return (
    <div
      style={{ ...row, fontSize: 11 }}
      title={
        zh
          ? `${model.provider} · ${model.name}\n${integer(model.selectedTokens)} 已选 Token\n按 API 价格估算 ${costText(model.cost, zh)}`
          : `${model.provider} · ${model.name}\n${integer(model.selectedTokens)} selected tokens\n${costText(model.cost, zh)} at API prices`
      }
    >
      <Swatch color={modelColor(model.id)} />
      <span style={truncate}>{model.name}</span>
      <span style={{ marginLeft: "auto", ...number }}>
        {compact(model.selectedTokens)}
      </span>
      <span
        style={{
          width: 34,
          textAlign: "right",
          color: theme.secondary,
          ...number,
        }}
      >
        {total > 0 ? share(model.selectedTokens, total) : "—"}
      </span>
    </div>
  );
}

export function ModelBar({
  models,
  total,
  theme,
}: {
  models: StatisticsModel[];
  total: number;
  theme: StatsTheme;
}) {
  return (
    <div
      aria-hidden
      style={{
        display: "flex",
        height: 5,
        overflow: "hidden",
        background: theme.track,
        borderRadius: 3,
      }}
    >
      {models.map((model) => (
        <span
          key={model.id}
          style={{
            flex: `${total > 0 ? model.selectedTokens / total : 0} 1 0`,
            minWidth: 0,
            background: modelColor(model.id),
            transition:
              "flex-grow 240ms cubic-bezier(0.22, 1, 0.36, 1), background-color 200ms ease",
          }}
        />
      ))}
    </div>
  );
}

export function ModelList({
  models,
  total,
  theme,
}: {
  models: StatisticsModel[];
  total: number;
  theme: StatsTheme;
}) {
  const [expanded, setExpanded] = useState(false);
  const zh = Boolean(theme.zh);
  return (
    <div style={{ display: "grid", gap: 5, minWidth: 0, flex: 1 }}>
      <ModelBar models={models} total={total} theme={theme} />
      {(expanded ? models : models.slice(0, 3)).map((model) => (
        <ModelLine key={model.id} model={model} total={total} theme={theme} />
      ))}
      {models.length > 3 && (
        <button
          type="button"
          onClick={() => setExpanded((value) => !value)}
          style={{ ...plain, color: theme.secondary, fontSize: 11 }}
        >
          {expanded
            ? zh
              ? "收起模型"
              : "Show fewer models"
            : zh
              ? `其余 ${models.length - 3} 个模型`
              : `${models.length - 3} more models`}
        </button>
      )}
    </div>
  );
}
