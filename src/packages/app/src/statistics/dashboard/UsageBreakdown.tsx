import { HUDArtwork as HUDLogo } from "@agent-hud/hud/HUDArtwork";
import type { StatisticsResponse } from "@agent-hud/protocol";
import {
  compact,
  modelColor,
  number,
  row,
  truncate,
  type StatsTheme,
} from "../StatsTheme";

const AGENTS = 4;
const MODELS = 3;

/** A stretch's tokens on a hover card, by agent and then model: each agent
 * with its share, its top models under it; the picked agents only. */
export function UsageBreakdown({
  models,
  available,
  agents,
  theme,
}: {
  /** Counted tokens by provider-qualified model id. */
  models: Record<string, number> | undefined;
  available: StatisticsResponse["available"];
  agents: string[] | null;
  theme: StatsTheme;
}) {
  const zh = Boolean(theme.zh);
  const catalog = new Map(available.models.map((model) => [model.id, model]));
  const names = new Map(
    available.providers.map((provider) => [provider.id, provider.name]),
  );
  const groups = new Map<
    string,
    { total: number; models: [string, number][] }
  >();
  for (const [id, tokens] of Object.entries(models ?? {})) {
    if (tokens <= 0) continue;
    const agent = catalog.get(id)?.provider ?? id.split("-model:")[0];
    if (agents && !agents.includes(agent)) continue;
    const group = groups.get(agent) ?? { total: 0, models: [] };
    group.total += tokens;
    group.models.push([id, tokens]);
    groups.set(agent, group);
  }
  const sorted = [...groups].sort((a, b) => b[1].total - a[1].total);
  const total = sorted.reduce((sum, [, group]) => sum + group.total, 0);
  if (!sorted.length) return null;
  return (
    <div style={{ display: "grid", gap: 6, marginTop: 2 }}>
      {sorted.slice(0, AGENTS).map(([agent, group]) => {
        const shown = group.models.sort((a, b) => b[1] - a[1]).slice(0, MODELS);
        return (
          <div key={agent} style={{ display: "grid", gap: 2 }}>
            <div style={{ ...row, gap: 6, fontWeight: 600 }}>
              <HUDLogo source={agent} size={12} light={theme.light} />
              <span style={truncate}>{names.get(agent) ?? agent}</span>
              <span style={{ ...number, marginLeft: "auto" }}>
                {compact(group.total)}
                <span style={{ fontWeight: 400, color: theme.secondary }}>
                  {" "}
                  {Math.round((group.total / Math.max(1, total)) * 100)}%
                </span>
              </span>
            </div>
            {shown.map(([id, tokens]) => (
              <div
                key={id}
                style={{
                  ...row,
                  gap: 6,
                  paddingLeft: 18,
                  color: theme.secondary,
                }}
              >
                <span
                  aria-hidden
                  style={{
                    width: 6,
                    height: 6,
                    borderRadius: 3,
                    flex: "none",
                    background: modelColor(id),
                  }}
                />
                <span style={truncate}>{catalog.get(id)?.name ?? id}</span>
                <span style={{ ...number, marginLeft: "auto" }}>
                  {compact(tokens)}
                </span>
              </div>
            ))}
            {group.models.length > MODELS && (
              <span
                style={{ paddingLeft: 18, color: theme.tertiary, fontSize: 10 }}
              >
                {zh
                  ? `另有 ${group.models.length - MODELS} 个模型`
                  : `${group.models.length - MODELS} more models`}
              </span>
            )}
          </div>
        );
      })}
      {sorted.length > AGENTS && (
        <span style={{ color: theme.tertiary, fontSize: 10 }}>
          {zh
            ? `另有 ${sorted.length - AGENTS} 个 Agent`
            : `${sorted.length - AGENTS} more agents`}
        </span>
      )}
    </div>
  );
}
