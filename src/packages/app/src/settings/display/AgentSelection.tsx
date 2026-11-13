/* The agents this screen's HUD shows, one switch each, in either mode. */
import type { Snapshot } from "@agent-hud/protocol";
import { vendorKey, vendorsFromSnapshot } from "@agent-hud/hud/model";
import { HUDArtwork } from "@agent-hud/hud/HUDArtwork";
import { Switch } from "@agent-hud/ui/components/switch";
import { MoreRows, Row } from "../SettingsPrimitives";
import { useLightTheme } from "../agent-plugins/use-status";
import { shownAgents, toggledAgents } from "./agent-selection";
import type { DisplayProps } from "./controls";

export function AgentSelection({
  snapshot,
  config,
  update,
  t,
}: DisplayProps & { snapshot: Snapshot }) {
  const light = useLightTheme();
  const agents = vendorsFromSnapshot(snapshot, config.mode);
  if (agents.length < 2) return null;
  const available = agents.map((agent) => agent.key);
  const saved = (config.panelLayout.hud?.agents ?? []).map(vendorKey);
  const shown = shownAgents(available, saved);
  return (
    <MoreRows label={t("Agents on this HUD", "这块屏幕的 HUD 显示哪些智能体")}>
      {agents.map((agent) => {
        const on = shown.includes(agent.key);
        const last = on && shown.length === 1;
        return (
          <Row
            key={agent.key}
            title={agent.name}
            subtitle={
              last
                ? t("One agent always stays.", "至少留一个智能体。")
                : undefined
            }
            icon={<HUDArtwork source={agent.name} size={18} light={light} />}
          >
            <Switch
              aria-label={t(`Show ${agent.name}`, `显示 ${agent.name}`)}
              checked={on}
              disabled={last}
              onCheckedChange={() =>
                update({
                  panelLayout: {
                    ...config.panelLayout,
                    hud: {
                      ...config.panelLayout.hud,
                      agents: toggledAgents(available, saved, agent.key),
                    },
                  },
                })
              }
            />
          </Row>
        );
      })}
    </MoreRows>
  );
}
