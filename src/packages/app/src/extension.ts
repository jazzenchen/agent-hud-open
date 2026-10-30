/** What a product built on Agent HUD uses to add its own Settings pages and
 * agent plugin rows to the app. */
export type { AppExtensionPage, AppProps } from "./App";
export type {
  AgentPluginContext,
  AgentPluginItem,
} from "./settings/AgentPluginsPane";
export { PluginControl, ToolSwitchRow } from "./settings/AgentPluginsPane";
export { useOpenSettingsPage } from "./settings/SettingsNavigation";
/** The Settings building blocks, so a product's pages look like the rest. */
export {
  Card,
  Collapsible,
  Footnote,
  LinkButton,
  Menu,
  MoreRows,
  Notice,
  Page,
  Pill,
  Row,
  Section,
  Segments,
  Slide,
  SmallButton,
  ToggleRow,
  Unavailable,
  useText,
  type PillTone,
  type Text,
} from "./settings/SettingsPrimitives";
