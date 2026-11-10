import type { Snapshot } from "@agent-hud/protocol";
import { vendorsFromSnapshot, vendorKey } from "@agent-hud/hud/model";
import {
  selectedAgentKeys,
  toggleAgentKey,
} from "@agent-hud/hud/agent-selection";
import { HUDArtwork } from "@agent-hud/hud/HUDArtwork";
import { Switch } from "@agent-hud/ui/components/switch";
import { MoreRows, Row } from "../SettingsPrimitives";
import { useLightTheme } from "../agent-plugins/use-status";
import type { DisplayProps } from "./controls";

export function AgentSelection({
  snapshot,
  config,
  update,
  t,
}: DisplayProps & { snapshot: Snapshot }) {
  const light = useLightTheme();
  const agents = vendorsFromSnapshot(snapshot, "simple");
  const available = agents.map((agent) => agent.key);
  const saved = (config.panelLayout.hud?.agents ?? []).map(vendorKey);
  const selected = selectedAgentKeys(available, saved);
  if (!agents.length) return null;
  return (
    <MoreRows label={t("AIs on the status bar", "显示的 AI")}>
      {agents.map((agent) => {
        const checked = selected.includes(agent.key);
        const last = checked && selected.length === 1;
        return (
          <Row
            key={agent.key}
            title={agent.name}
            subtitle={
              last
                ? t("Keep at least one AI visible.", "至少保留一个 AI。")
                : undefined
            }
            icon={<HUDArtwork source={agent.name} size={18} light={light} />}
          >
            <Switch
              aria-label={t(`Show ${agent.name}`, `显示 ${agent.name}`)}
              checked={checked}
              disabled={last}
              onCheckedChange={() =>
                update({
                  panelLayout: {
                    ...config.panelLayout,
                    hud: {
                      ...config.panelLayout.hud,
                      agents: toggleAgentKey(available, saved, agent.key),
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
