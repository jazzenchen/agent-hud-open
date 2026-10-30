// What a product build adds to Settings → Agent plugins, and what each of its
// rows is given to draw itself.
import type { ReactNode } from "react";
import type { Snapshot } from "@agent-hud/protocol";

/** What a product build's row is given to draw itself in a client's card. */
export interface AgentPluginContext {
  snapshot: Snapshot;
  /** The product's own requests (the "extension" method). */
  request: (method: string, params?: unknown) => Promise<unknown>;
  /** Whether the Settings window is on screen: poll only while it is. */
  active: boolean;
  /** Whether the client's command line is on this computer; null while unknown. */
  clientFound: boolean | null;
  /** The client's Agent HUD plugin (Claude Code's or DeepSeek Harness's);
   * installed is null while it is read, or when the client has none. */
  plugin: { installed: boolean | null };
}

/** What a product build adds to one client's card on Agent plugins. */
export interface AgentPluginItem {
  id: string;
  /** The card it joins: "claudeCode", "codex", or a client of its own. */
  client: string;
  /** "plugin": a switch under the client's Agent HUD plugin row (Claude
   * Code's or DeepSeek Harness's), after Open's own, drawn with
   * `PluginControl`; shown once the plugin is installed. "row" (the
   * default): rows of its own in the client's card, before the HUD tools
   * row. "tools": a tool group of the product's own in the Agent tools
   * section, after the HUD switch, drawn with `ToolSwitchRow`; `client` is
   * not read. */
  place?: "row" | "plugin" | "tools";
  /** The card's name when Open has no card for this client. */
  clientName?: string;
  render(context: AgentPluginContext): ReactNode;
}
