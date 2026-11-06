import { useState } from "react";
import type {
  StatisticsProvider,
  StatisticsResponse,
} from "@agent-hud/protocol";
import { compact, costShort, plain, type StatsTheme } from "../StatsTheme";
import { agentColor } from "./colors";
import { Donut, LegendRow } from "./Donut";
import { Panel } from "./Panel";
import { Tween } from "../motion";

/** Agents the legend names; the rest fold into one line. */
const SHOWN = 5;
import { Bar } from "./Placeholder";

/** Where the range's tokens went: a donut of the agents' shares beside a
 * legend with each one's tokens, worth and sessions. Picking an agent shows
 * its models the same way; with an agent filtered, its models show. */
export function AgentBreakdown({
  data,
  agent,
  theme,
  light,
}: {
  data: StatisticsResponse | null;
  agent: string | null;
  theme: StatsTheme;
  light: boolean;
}) {
  const zh = Boolean(theme.zh);
  const t = (en: string, cn: string) => (zh ? cn : en);
  const [open, setOpen] = useState<string | null>(null);
  const [hovered, setHovered] = useState<string | null>(null);
  const providers = (data?.providers ?? []).filter(
    (provider) => provider.selectedTokens > 0,
  );
  const opened: StatisticsProvider | undefined = providers.find(
    (provider) => provider.id === (agent ?? open),
  );
  const total = Math.max(1, data?.totals.selectedTokens ?? 0);
  const sessions = (count: number | undefined) =>
    count === undefined
      ? ""
      : zh
        ? ` · ${count} 个会话`
        : ` · ${count} ${count === 1 ? "session" : "sessions"}`;
  // An agent's models in its colour, a step lighter each, largest first.
  const ground = light ? "#fff" : "#2e2e2e";
  const models = opened
    ? opened.models
        .filter((model) => model.selectedTokens > 0)
        .slice(0, 6)
        .map((model, index) => ({
          id: model.id,
          name: model.name,
          value: model.selectedTokens,
          color: `color-mix(in srgb, ${agentColor(opened.id)} ${100 - index * 15}%, ${ground})`,
          detail: costShort(model.cost, zh),
        }))
    : [];
  const parts = opened
    ? models
    : providers.map((provider) => ({
        id: provider.id,
        value: provider.selectedTokens,
        color: agentColor(provider.id),
      }));
  const whole = opened ? Math.max(1, opened.selectedTokens) : total;
  // What the agents past the legend's last line add up to, at least 1%.
  const rest = Math.max(
    1,
    Math.round(
      (providers
        .slice(SHOWN)
        .reduce((sum, provider) => sum + provider.selectedTokens, 0) /
        total) *
        100,
    ),
  );
  return (
    <Panel
      area="agents"
      theme={theme}
      style={{ gridTemplateRows: "auto 1fr", alignContent: "stretch" }}
      title={
        opened
          ? `${opened.name} · ${t("models", "模型")}`
          : t("Share by agent", "各 Agent 占比")
      }
      meta={
        opened
          ? t("Share of its tokens", "占它的 Token")
          : t(
              "Share of tokens · click an agent for its models",
              "Token 占比 · 点一个 Agent 看它的模型",
            )
      }
      actions={
        opened && !agent ? (
          <button
            type="button"
            onClick={() => setOpen(null)}
            style={{ ...plain, fontSize: 11, color: theme.secondary }}
          >
            {t("All agents", "全部 Agent")}
          </button>
        ) : undefined
      }
    >
      {!data ? (
        <div className="stats-donut-layout">
          <Bar
            width={132}
            height={132}
            radius={66}
            theme={theme}
            style={{
              background: "transparent",
              border: `18px solid ${theme.track}`,
              boxSizing: "border-box",
            }}
          />
          <div style={{ display: "grid", gap: 14 }}>
            {[0, 1, 2, 3].map((index) => (
              <div key={index} style={{ display: "grid", gap: 5 }}>
                <Bar width="70%" height={11} theme={theme} />
                <Bar width="45%" height={9} theme={theme} />
              </div>
            ))}
          </div>
        </div>
      ) : !providers.length ? (
        <span style={{ fontSize: 12, color: theme.secondary }}>
          {t("No usage in this range", "这段时间没有用量")}
        </span>
      ) : (
        <div className="stats-donut-layout stats-reveal">
          <Donut
            parts={parts}
            total={whole}
            hovered={hovered}
            setHovered={setHovered}
            center={
              <Tween
                value={
                  opened
                    ? opened.selectedTokens
                    : (data?.totals.selectedTokens ?? 0)
                }
                format={(value) => compact(Math.round(value))}
              />
            }
            caption={costShort(
              opened ? opened.cost : (data?.totals.cost ?? null),
              zh,
            )}
            theme={theme}
          />
          <div
            style={{
              display: "grid",
              gap: 2,
              alignContent: "start",
              minWidth: 0,
            }}
          >
            {opened
              ? models.map((model) => (
                  <LegendRow
                    key={model.id}
                    id={model.id}
                    name={model.name}
                    color={model.color}
                    share={model.value / whole}
                    value={
                      <Tween
                        value={model.value}
                        format={(value) => compact(Math.round(value))}
                      />
                    }
                    detail={model.detail}
                    logo={false}
                    hovered={hovered}
                    setHovered={setHovered}
                    theme={theme}
                    light={light}
                  />
                ))
              : providers
                  .slice(0, SHOWN)
                  .map((provider) => (
                    <LegendRow
                      key={provider.id}
                      id={provider.id}
                      name={provider.name}
                      color={agentColor(provider.id)}
                      share={provider.selectedTokens / total}
                      value={
                        <Tween
                          value={provider.selectedTokens}
                          format={(value) => compact(Math.round(value))}
                        />
                      }
                      detail={`${costShort(provider.cost, zh)}${sessions(provider.sessionCount)}`}
                      logo
                      hovered={hovered}
                      setHovered={setHovered}
                      onClick={() => setOpen(provider.id)}
                      theme={theme}
                      light={light}
                    />
                  ))}
            {!opened && providers.length > SHOWN && (
              <span
                style={{
                  fontSize: 11,
                  color: theme.secondary,
                  padding: "2px 6px 0 24px",
                }}
              >
                {t(
                  `${providers.length - SHOWN} more, ${rest}% together`,
                  `另有 ${providers.length - SHOWN} 个，合计 ${rest}%`,
                )}
              </span>
            )}
          </div>
        </div>
      )}
    </Panel>
  );
}
