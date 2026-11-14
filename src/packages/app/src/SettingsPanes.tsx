import type { PanelLayout, SourcePreferences } from "@agent-hud/protocol";
import type { NativeHUDMonitor } from "./lib/use-hud-geometry";
import AgentsPage from "./settings/agents/AgentsPage";
import { useT } from "@agent-hud/ui/format-context";
import { LoadingState } from "./LoadingState";
import {
  AgentPluginsPane,
  type AgentPluginItem,
} from "./settings/AgentPluginsPane";
import { DisplayPane } from "./settings/DisplayPane";
import { GeneralPane } from "./settings/GeneralPane";
import { LocalNetworkPane } from "./settings/LocalNetworkPane";
import { PricesPane } from "./settings/PricesPane";
import { RelaysPane } from "./settings/RelaysPane";
import { UpdatesPane } from "./settings/UpdatesPane";
import type { SettingsPaneProps } from "./settings/SettingsPrimitives";

export default function SettingsPane({
  page,
  settingsProps,
  ready,
  selectedId,
  monitors,
  selectScreen,
  layout,
  onLayout,
  preferences,
  onPreferences,
  agentPlugins = [],
  request,
}: {
  page: string;
  settingsProps: SettingsPaneProps;
  ready: boolean;
  selectedId: string;
  monitors: NativeHUDMonitor[];
  selectScreen: (id: string) => void;
  layout: PanelLayout;
  onLayout: (layout: PanelLayout) => Promise<unknown>;
  preferences: Record<string, SourcePreferences>;
  onPreferences: (source: string, patch: SourcePreferences) => Promise<unknown>;
  agentPlugins?: AgentPluginItem[];
  request: (method: string, params?: unknown) => Promise<unknown>;
}) {
  const t = useT();
  if (page === "general") return <GeneralPane {...settingsProps} />;
  if (page === "prices") return <PricesPane {...settingsProps} />;
  if (page === "relays") return <RelaysPane {...settingsProps} />;
  if (page === "local-network") return <LocalNetworkPane {...settingsProps} />;
  if (page === "updates") return <UpdatesPane {...settingsProps} />;
  if (page === "display")
    return ready && selectedId ? (
      <DisplayPane
        key={selectedId}
        {...settingsProps}
        monitors={monitors}
        selectedScreenId={selectedId}
        onSelectScreen={selectScreen}
      />
    ) : (
      <LoadingState label={t("Reading displays…")} />
    );
  if (page === "agents")
    return (
      <AgentsPage
        snapshot={settingsProps.snapshot}
        action={settingsProps.action}
        active={settingsProps.active}
        layout={layout}
        onLayout={onLayout}
        preferences={preferences}
        onPreferences={onPreferences}
      />
    );
  if (page === "agent-plugins")
    return (
      <AgentPluginsPane
        snapshot={settingsProps.snapshot}
        action={settingsProps.action}
        active={settingsProps.active}
        items={agentPlugins}
        request={request}
      />
    );
  return null;
}
