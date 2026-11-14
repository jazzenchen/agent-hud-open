import type { CSSProperties } from "react";
import { useT } from "@agent-hud/ui/format-context";
import { Check, Minus } from "lucide-react";
import { HUDArtwork as HUDLogo } from "@agent-hud/hud/HUDArtwork";
import type { FilterChoice } from "@agent-hud/protocol";
import { number, plain, row, truncate, type StatsTheme } from "./StatsTheme";
import type { FilterState } from "./section-filter";

/** A model as a filter lists it, under the agent it belongs to. */
export interface ModelOption {
  id: string;
  name: string;
  /** The agent it is listed under. */
  agent: string;
  /** Its colour where a chart draws it in that colour. */
  color?: string;
  /** A figure at the end of its line, such as its tokens. */
  figure?: string;
}

/** One agent's models, in the order they were given. */
export interface ModelGroup {
  agent: FilterChoice;
  models: ModelOption[];
}

/** The agents' models, under the agents in `agents`' order; an agent the
 * models name but `agents` does not comes last, by its id. */
export function modelGroups(
  agents: FilterChoice[],
  models: ModelOption[],
): ModelGroup[] {
  const groups = new Map<string, ModelGroup>(
    agents.map((agent) => [agent.id, { agent, models: [] }]),
  );
  for (const model of models) {
    const group = groups.get(model.agent) ?? {
      agent: { id: model.agent, name: model.agent },
      models: [],
    };
    if (!group.models.some((entry) => entry.id === model.id))
      group.models.push(model);
    groups.set(model.agent, group);
  }
  return [...groups.values()].filter((group) => group.models.length > 0);
}

function Box({
  state,
  theme,
}: {
  state: boolean | "mixed";
  theme: StatsTheme;
}) {
  const on = state !== false;
  return (
    <span
      aria-hidden
      style={{
        width: 13,
        height: 13,
        flexShrink: 0,
        borderRadius: 3.5,
        border: `1px solid ${on ? "#007aff" : theme.divider}`,
        background: on ? "#007aff" : "transparent",
        color: "#fff",
        display: "inline-grid",
        placeItems: "center",
      }}
    >
      {state === "mixed" ? (
        <Minus size={9} strokeWidth={3} />
      ) : (
        state && <Check size={9} strokeWidth={3} />
      )}
    </span>
  );
}

/** A section's model checklist, folded under its Models button: each shown
 * agent's models under its name, where one click shows or hides them all. */
export function ModelFilter({
  expanded,
  groups,
  state,
  loading,
  theme,
  light,
}: {
  expanded: boolean;
  groups: ModelGroup[];
  state: FilterState;
  loading?: boolean;
  theme: StatsTheme;
  light: boolean;
}) {
  const t = useT();
  const ids = [
    ...new Set(groups.flatMap((group) => group.models.map(({ id }) => id))),
  ];
  const everything = ids.every(state.modelShown);
  const line = (on: boolean): CSSProperties => ({
    ...plain,
    ...row,
    width: "100%",
    textAlign: "left",
    color: on ? theme.text : theme.secondary,
    fontSize: 11,
    padding: "3px 4px",
    borderRadius: 4,
  });
  return (
    <div
      style={{
        display: "grid",
        gridTemplateRows: expanded ? "1fr" : "0fr",
        opacity: expanded ? 1 : 0,
        visibility: expanded ? "visible" : "hidden",
        pointerEvents: expanded ? "auto" : "none",
        transition:
          "grid-template-rows 200ms cubic-bezier(0.22, 1, 0.36, 1), opacity 180ms ease",
      }}
    >
      <div style={{ overflow: "hidden", minHeight: 0 }}>
        <div style={{ display: "grid", gap: 8, paddingTop: 8 }}>
          {ids.length > 0 && (
            <button
              type="button"
              onClick={() => state.setModels(ids, true)}
              style={{ ...plain, ...row, color: theme.secondary, fontSize: 11 }}
            >
              <Check size={11} style={{ opacity: everything ? 1 : 0 }} />
              {t("All models")}
              {!everything && ` · ${t("Show all")}`}
            </button>
          )}
          {groups.map(({ agent, models }) => {
            const shown = models.filter((model) => state.modelShown(model.id));
            const all = shown.length === models.length;
            const mark = all ? true : shown.length ? "mixed" : false;
            return (
              <div key={agent.id} style={{ display: "grid", gap: 2 }}>
                <button
                  type="button"
                  role="checkbox"
                  aria-checked={mark}
                  aria-label={t("Show {{agent}}'s models", {
                    agent: agent.name,
                  })}
                  onClick={() =>
                    state.setModels(
                      models.map((model) => model.id),
                      !all,
                    )
                  }
                  style={{ ...line(true), fontWeight: 600 }}
                >
                  <Box state={mark} theme={theme} />
                  <HUDLogo source={agent.id} size={12} light={light} />
                  <span style={truncate}>{agent.name}</span>
                  <span
                    style={{
                      marginLeft: "auto",
                      color: theme.secondary,
                      fontWeight: 400,
                      ...number,
                    }}
                  >
                    {shown.length}/{models.length}
                  </span>
                </button>
                <div
                  style={{
                    display: "grid",
                    gridTemplateColumns: "repeat(auto-fit,minmax(190px,1fr))",
                    gap: "2px 16px",
                    paddingLeft: 17,
                  }}
                >
                  {models.map((model) => {
                    const checked = state.modelShown(model.id);
                    return (
                      <button
                        key={model.id}
                        type="button"
                        role="checkbox"
                        aria-checked={checked}
                        aria-label={t("Show {{model}} from {{agent}}", {
                          model: model.name,
                          agent: agent.name,
                        })}
                        title={`${agent.name} · ${model.name}`}
                        onClick={() => state.setModels([model.id], !checked)}
                        style={{
                          ...line(checked),
                          background: checked ? theme.segment : "transparent",
                        }}
                      >
                        <Box state={checked} theme={theme} />
                        {model.color && (
                          <span
                            aria-hidden
                            style={{
                              width: 6,
                              height: 6,
                              flexShrink: 0,
                              borderRadius: 2,
                              background: model.color,
                            }}
                          />
                        )}
                        <span style={truncate}>{model.name}</span>
                        {model.figure && (
                          <span style={{ marginLeft: "auto", ...number }}>
                            {model.figure}
                          </span>
                        )}
                      </button>
                    );
                  })}
                </div>
              </div>
            );
          })}
          {!groups.length && (
            <span style={{ color: theme.secondary, fontSize: 11 }}>
              {loading ? t("Loading models…") : t("No recorded models")}
            </span>
          )}
        </div>
      </div>
    </div>
  );
}
