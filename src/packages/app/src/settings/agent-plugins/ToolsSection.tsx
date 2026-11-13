// Agent tools: the app-wide switches for what agents may do with Agent HUD's
// tools: arrange the HUD, and message each other. A product build may add
// groups of its own.
import { Fragment } from "react";
import type { Snapshot } from "@agent-hud/protocol";
import {
  Section,
  type SettingsPaneProps,
  type Text,
} from "../SettingsPrimitives";
import { AgentMessagesRow } from "./AgentMessagesRow";
import { ToolSwitchRow } from "./rows";
import type { AgentPluginContext, AgentPluginItem } from "./types";
import type { HudTools } from "./use-status";

export function ToolsSection({
  tools,
  groups,
  snapshot,
  action,
  request,
  active,
  t,
}: {
  tools: HudTools;
  groups: AgentPluginItem[];
  snapshot: Snapshot;
  action: SettingsPaneProps["action"];
  request: AgentPluginContext["request"];
  active: boolean;
  t: Text;
}) {
  const { unavailable, problem } = tools.state;
  return (
    <Section title={t("Agent tools", "智能体工具")}>
      <ToolSwitchRow
        anchor="hud-tools"
        title={t("Let agents arrange the HUD", "允许智能体调整 HUD")}
        description={t(
          "Changes stay drafts until you save them in the panel.",
          "改动先是草稿，在面板里保存后才生效。",
        )}
        checked={tools.on}
        pending={tools.pending}
        unavailable={unavailable ? t(...unavailable) : null}
        error={tools.error ?? (problem ? t(...problem) : null)}
        onChange={(value) => void tools.set(value)}
      />
      <AgentMessagesRow snapshot={snapshot} action={action} t={t} />
      {groups.map((item) => (
        <Fragment key={item.id}>
          {item.render({
            snapshot,
            request,
            active,
            clientFound: null,
            plugin: { installed: null },
          })}
        </Fragment>
      ))}
    </Section>
  );
}
