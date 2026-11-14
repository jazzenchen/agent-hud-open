import { useState } from "react";
import { ChevronDown, ChevronRight } from "lucide-react";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@agent-hud/ui/components/dropdown-menu";
import { HUDArtwork as HUDLogo } from "@agent-hud/hud/HUDArtwork";
import type { FilterChoice } from "@agent-hud/protocol";
import { useT } from "@agent-hud/ui/format-context";
import { plain, row, type StatsTheme } from "./StatsTheme";
import { ModelFilter, modelGroups, type ModelOption } from "./ModelFilter";
import type { FilterState } from "./section-filter";

/** A statistics section's filter line: the Models button that folds out the
 * models grouped by agent, the Agents menu, and Clear filter while anything
 * is left out. Only the shown agents' models are listed. */
export function SectionFilter({
  agents,
  models,
  state,
  loading,
  theme,
  light,
}: {
  agents: FilterChoice[];
  models: ModelOption[];
  state: FilterState;
  loading?: boolean;
  theme: StatsTheme;
  light: boolean;
}) {
  const t = useT();
  const [expanded, setExpanded] = useState(false);
  const groups = modelGroups(agents, models).filter((group) =>
    state.agentShown(group.agent.id),
  );
  const agentIds = agents.map((agent) => agent.id);
  const modelIds = [...new Set(models.map((model) => model.id))];
  const shownAgents = agentIds.filter(state.agentShown).length;
  const shownModels = modelIds.filter(state.modelShown).length;
  const filtered =
    shownAgents < agentIds.length || shownModels < modelIds.length;
  const count = (shown: number, total: number) =>
    shown === total ? t("All") : t("{{shown}} of {{total}}", { shown, total });
  return (
    <>
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
            onClick={() => setExpanded((value) => !value)}
            aria-expanded={expanded}
            style={{ ...plain, ...row, gap: 4 }}
          >
            <ChevronRight
              size={10}
              style={{
                transform: expanded ? "rotate(90deg)" : "rotate(0deg)",
                transition: "transform 180ms cubic-bezier(0.22, 1, 0.36, 1)",
              }}
            />
            {t("Models")} · {count(shownModels, modelIds.length)}
          </button>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <button
                type="button"
                style={{ ...plain, ...row, gap: 4 }}
                aria-label={t("Filter by agent")}
              >
                {t("Agents")} · {count(shownAgents, agentIds.length)}
                <ChevronDown size={10} />
              </button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start">
              <DropdownMenuCheckboxItem
                checked={shownAgents === agentIds.length}
                onSelect={(event) => event.preventDefault()}
                onCheckedChange={() => state.setAgents(agentIds, true)}
              >
                {t("All agents")}
              </DropdownMenuCheckboxItem>
              <DropdownMenuSeparator />
              {agents.map((agent) => (
                <DropdownMenuCheckboxItem
                  key={agent.id}
                  checked={state.agentShown(agent.id)}
                  onSelect={(event) => event.preventDefault()}
                  onCheckedChange={(checked) =>
                    state.setAgents([agent.id], Boolean(checked))
                  }
                >
                  <HUDLogo source={agent.id} size={12} light={light} />
                  {agent.name}
                </DropdownMenuCheckboxItem>
              ))}
              {!agents.length && (
                <span
                  style={{ padding: 8, color: theme.secondary, fontSize: 11 }}
                >
                  {t("No recorded agents")}
                </span>
              )}
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
        {filtered && (
          <button
            type="button"
            onClick={state.clear}
            style={{ ...plain, color: theme.secondary, fontSize: 11 }}
          >
            {t("Clear filter")}
          </button>
        )}
      </div>
      <ModelFilter
        expanded={expanded}
        groups={groups}
        state={state}
        loading={loading}
        theme={theme}
        light={light}
      />
    </>
  );
}
