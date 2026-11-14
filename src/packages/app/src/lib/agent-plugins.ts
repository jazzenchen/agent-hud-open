// The rows of Settings → Agent plugins as data: each row's status pill, its
// one action, what its ⋯ menu offers and the next step under it, in the
// interface language. The page draws them; a product build's own rows take
// their words from here too, so both builds read the same.
import type { Translate } from "@agent-hud/ui/format";

/** Text in the interface language. */
export type Words = string;

/** working: in use. installed: in place, not in use yet. attention: one step
 * left. off: not set up (or still being read). failed: it broke. */
export type Tone = "working" | "installed" | "attention" | "off" | "failed";

export interface RowAction {
  id: string;
  label: Words;
  /** Why it cannot run now; the button is drawn disabled with this as its tip. */
  unavailable?: Words;
}

export interface RowState {
  tone: Tone;
  status: Words;
  /** The one action that matters now, or none when the item is working. */
  action: RowAction | null;
  /** A command is running: its words replace the action. */
  busy: Words | null;
  /** The ⋯ menu; removing is always here, never the main button. */
  menu: { id: string; label: Words; destructive?: boolean }[];
  /** The next step, shown under the description only when there is one. A
   * command in it is drawn as code with a Copy button. */
  next: { words: Words; command?: string } | null;
}

export function sessionCount(t: Translate, count: number): Words {
  return t("Loaded in {{count}} sessions", { count });
}

export const reloadCommand = "/reload-plugins";

/** The line under a row whose command failed: "Couldn't install: …". */
export function failure(t: Translate, action: string, message: string): Words {
  const line = message.replace(/^Error:\s*/, "");
  switch (action) {
    case "install":
      return t("Couldn't install: {{message}}", { message: line });
    case "remove":
      return t("Couldn't remove: {{message}}", { message: line });
    case "add":
      return t("Couldn't add: {{message}}", { message: line });
    case "turn-on":
      return t("Couldn't turn on: {{message}}", { message: line });
    default:
      return t("Couldn't finish: {{message}}", { message: line });
  }
}

export interface PluginInput {
  /** null while it is being read. */
  installed: boolean | null;
  /** Sessions that loaded it and checked in lately. */
  sessions: number;
  /** Whether the client's command line is on this Mac; null when unknown. */
  clientFound: boolean | null;
  running: "install" | "remove" | null;
  /** Installed from this page since it opened. */
  justInstalled: boolean;
}

/** A plugin a client loads per session, such as Agent HUD's Claude Code
 * plugin. `live`: the client loads it into what is already running by itself,
 * as DeepSeek Harness does, so the next step is only to open it. */
export function pluginRow(
  t: Translate,
  input: PluginInput,
  client = "Claude Code",
  live = false,
): RowState {
  const base: Pick<RowState, "busy" | "menu" | "next"> = {
    busy: null,
    menu: [],
    next: null,
  };
  const busy: Words | null =
    input.running === "install"
      ? t("Installing…")
      : input.running === "remove"
        ? t("Removing…")
        : null;
  if (input.installed === null && !busy)
    return { ...base, tone: "off", status: t("Checking…"), action: null };
  if (!input.installed)
    return {
      ...base,
      tone: "off",
      status: t("Not installed"),
      busy,
      action: {
        id: "install",
        label: t("Install"),
        unavailable:
          input.clientFound === false
            ? t("Install {{client}} first.", { client })
            : undefined,
      },
    };
  const loaded = input.sessions > 0;
  return {
    tone: loaded ? "working" : "installed",
    status: loaded ? sessionCount(t, input.sessions) : t("Installed"),
    action: null,
    busy,
    menu: [{ id: "remove", label: t("Remove plugin"), destructive: true }],
    next: live
      ? loaded
        ? null
        : {
            words: t(
              "{{client}} loads it by itself within seconds, open sessions included. Open {{client}} to use it.",
              { client },
            ),
          }
      : !loaded || input.justInstalled
        ? {
            words: t(
              "New sessions load it. In an open session, run {{command}}.",
              { command: reloadCommand },
            ),
            command: reloadCommand,
          }
        : null,
  };
}

export interface McpInput {
  /** The client's display name. */
  client: string;
  /** Whether this app may add the server (the installed app with its relay); null while reading. */
  setupReady: boolean | null;
  clientFound: boolean;
  added: boolean;
  /** Whether the client could be given a command to run by hand. */
  hasCommand: boolean;
  /** The app-side switch (settings.hudTools). */
  toolsOn: boolean;
  /** Why the switch cannot be turned on here (the demo, an unsupported system), if it cannot. */
  toolsUnavailable: Words | null;
  /** Whether the tools channel is listening now. */
  listening: boolean;
  running: "add" | "remove" | null;
  justAdded: boolean;
  /** The command was copied from this row a moment ago. */
  copied: boolean;
}

/** Agent HUD's MCP server in one client, with the app-side switch it needs. */
export function mcpRow(t: Translate, input: McpInput): RowState {
  const { client } = input;
  const busy: Words | null =
    input.running === "add"
      ? t("Adding…")
      : input.running === "remove"
        ? t("Removing…")
        : null;
  const menu: RowState["menu"] = [];
  if (input.hasCommand) menu.push({ id: "copy", label: t("Copy command") });
  if (input.added && input.setupReady)
    menu.push({
      id: "remove",
      label: t("Remove from {{client}}", { client }),
      destructive: true,
    });
  const copied = input.copied
    ? {
        words: t("Command copied. Paste it into Terminal."),
      }
    : null;
  if (input.setupReady === null && !busy)
    return {
      tone: "off",
      status: t("Checking…"),
      action: null,
      busy: null,
      menu: [],
      next: null,
    };
  if (!input.added) {
    const unavailable: Words | undefined = !input.setupReady
      ? t(
          "Adding HUD tools requires Agent HUD at its approved installation location and its MCP components.",
        )
      : !input.clientFound
        ? t("Install {{client}} first.", { client })
        : undefined;
    return {
      tone: "off",
      status: input.setupReady ? t("Not added") : t("Unavailable"),
      action: { id: "add", label: t("Add"), unavailable },
      busy,
      menu,
      next: copied,
    };
  }
  if (!input.toolsOn)
    return {
      tone: "attention",
      status: t("Tools off"),
      action: {
        id: "turn-on",
        label: t("Turn on"),
        unavailable: input.toolsUnavailable ?? undefined,
      },
      busy,
      menu,
      next: copied,
    };
  if (!input.listening)
    return {
      tone: "failed",
      status: t("Not running"),
      action: null,
      busy,
      menu,
      next: copied,
    };
  return {
    tone: "working",
    status: t("Ready"),
    action: null,
    busy,
    menu,
    next:
      copied ??
      (input.justAdded
        ? {
            words: t(
              "Start a new {{client}} session and ask it to change the HUD.",
              { client },
            ),
          }
        : null),
  };
}

export interface ToolsSwitchInput {
  supported: boolean;
  enabled: boolean;
  listening: boolean;
  notice: string | null;
  demo: boolean;
}

/** The app-side switch: why it cannot change, and a line only when something is wrong. */
export function toolsSwitch(
  t: Translate,
  input: ToolsSwitchInput,
): {
  unavailable: Words | null;
  problem: Words | null;
} {
  const unavailable: Words | null = input.demo
    ? t("Not available in the demo.")
    : !input.supported
      ? t("This system can't serve HUD tools yet.")
      : null;
  const problem: Words | null =
    input.supported && input.enabled && !input.listening
      ? input.notice
        ? t("Not running: {{notice}}", { notice: input.notice })
        : t("Not running.")
      : !input.supported && !input.demo
        ? unavailable
        : null;
  return { unavailable, problem };
}
