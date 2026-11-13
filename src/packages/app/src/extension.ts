/** What a product built on Agent HUD uses to add its own Settings pages and
 * agent plugin rows to the app. */
export type { AppProps } from "./App";
export type { AppExtensionPage } from "./settings/pages";
export type {
  NewsEntry,
  NewsItem,
  ProductGuide,
  SettingsStep,
} from "./guide/plan";
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
  MoreRows,
  Notice,
  Page,
  Pill,
  Row,
  Section,
  useText,
  type PillTone,
  type Text,
} from "./settings/SettingsPrimitives";
export {
  LinkButton,
  Menu,
  Segments,
  Slide,
  SmallButton,
  ToggleRow,
  Unavailable,
} from "./settings/SettingsControls";
