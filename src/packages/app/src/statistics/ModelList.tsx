import type { StatisticsModel } from "@agent-hud/protocol";
import { share } from "@agent-hud/ui/format";
import { useFormat, useT } from "@agent-hud/ui/format-context";
import {
  compact,
  integer,
  modelColor,
  number,
  row,
  truncate,
  type StatsTheme,
} from "./StatsTheme";
import { costText } from "./cost-text";
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
  const t = useT();
  const { language } = useFormat();
  return (
    <div
      style={{ ...row, fontSize: 11 }}
      title={t(
        "{{provider}} · {{model}}\n{{tokens}} selected tokens\n{{cost}} at API prices",
        {
          provider: model.provider,
          model: model.name,
          tokens: integer(model.selectedTokens),
          cost: costText(model.cost, language),
        },
      )}
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

/** The top three models, and the rest as one quiet line with their tokens
 * together; each name and figure is in the line's tooltip. */
export function ModelList({
  models,
  total,
  theme,
}: {
  models: StatisticsModel[];
  total: number;
  theme: StatsTheme;
}) {
  const t = useT();
  const rest = models.slice(3);
  const restTokens = rest.reduce((sum, model) => sum + model.selectedTokens, 0);
  return (
    <div style={{ display: "grid", gap: 5, minWidth: 0, flex: 1 }}>
      <ModelBar models={models} total={total} theme={theme} />
      {models.slice(0, 3).map((model) => (
        <ModelLine key={model.id} model={model} total={total} theme={theme} />
      ))}
      {rest.length > 0 && (
        <div
          style={{ ...row, fontSize: 11, color: theme.secondary }}
          title={rest
            .map((model) => `${model.name} · ${compact(model.selectedTokens)}`)
            .join("\n")}
        >
          <Swatch color="transparent" />
          <span style={truncate}>
            {t("{{count}} more models", { count: rest.length })}
          </span>
          <span style={{ marginLeft: "auto", ...number }}>
            {compact(restTokens)}
          </span>
          <span style={{ width: 34, textAlign: "right", ...number }}>
            {total > 0 ? share(restTokens, total) : "—"}
          </span>
        </div>
      )}
    </div>
  );
}
