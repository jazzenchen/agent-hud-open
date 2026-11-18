// What a product build adds to Settings → Agents (rows on a client's page,
// agent tools of its own), and what each of its rows is given to draw itself.
import type { ReactNode } from "react";
import type { Snapshot } from "@agent-hud/protocol";

/** What a product build's row is given to draw itself on a client's page. */
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

/** What a product build adds to one client's page on Agents. */
export interface AgentPluginItem {
  id: string;
  /** The client whose page it joins, by the host's id for it ("claudeCode",
   * "codex", hud_tools::CLIENTS); every page of that client shows it (Claude
   * Code CLI's and Claude Desktop's). */
  client: string;
  /** "plugin": a switch under the client's Agent HUD plugin row (Claude
   * Code's or DeepSeek Harness's), after Open's own, drawn with
   * `PluginControl`; shown once the plugin is installed. "row" (the
   * default): rows of its own in the client's "What it uses", before the
   * HUD tools row. "tools": a tool group of the product's own in the Agent
   * tools section, after the HUD switch, drawn with `ToolSwitchRow`;
   * `client` is not read. */
  place?: "row" | "plugin" | "tools";
  render(context: AgentPluginContext): ReactNode;
}
