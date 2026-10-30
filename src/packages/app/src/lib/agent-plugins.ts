// The rows of Settings → Agents → Agent plugins as data: each row's status
// chip, its one action, what its ⋯ menu offers and the next step under it, in
// English and Chinese. The page draws them; a product build's own rows take
// their words from here too, so both builds read the same.

export type Words = readonly [en: string, zh: string];

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

const checking: Words = ["Checking…", "正在检查…"];

export function sessionCount(count: number): Words {
  return [
    `Loaded in ${count} ${count === 1 ? "session" : "sessions"}`,
    `已在 ${count} 个会话中加载`,
  ];
}

export const reloadCommand = "/reload-plugins";

/** The line under a row whose command failed: "Couldn't install: …". */
export function failure(action: string, message: string): Words {
  const verbs: Record<string, Words> = {
    install: ["install", "安装"],
    remove: ["remove", "移除"],
    add: ["add", "添加"],
    "turn-on": ["turn on", "打开"],
  };
  const [en, zh] = verbs[action] ?? ["finish", "完成"];
  const line = message.replace(/^Error:\s*/, "");
  return [`Couldn't ${en}: ${line}`, `没能${zh}：${line}`];
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
      ? ["Installing…", "正在安装…"]
      : input.running === "remove"
        ? ["Removing…", "正在移除…"]
        : null;
  if (input.installed === null && !busy)
    return { ...base, tone: "off", status: checking, action: null };
  if (!input.installed)
    return {
      ...base,
      tone: "off",
      status: ["Not installed", "未安装"],
      busy,
      action: {
        id: "install",
        label: ["Install", "安装"],
        unavailable:
          input.clientFound === false
            ? [`Install ${client} first.`, `请先安装 ${client}。`]
            : undefined,
      },
    };
  const loaded = input.sessions > 0;
  return {
    tone: loaded ? "working" : "installed",
    status: loaded ? sessionCount(input.sessions) : ["Installed", "已安装"],
    action: null,
    busy,
    menu: [
      { id: "remove", label: ["Remove plugin", "移除插件"], destructive: true },
    ],
    next: live
      ? loaded
        ? null
        : {
            words: [
              `${client} loads it by itself within a few seconds, open sessions included. Open ${client} to use it.`,
              `${client} 会在几秒内自动加载，已经打开的会话也一样。打开 ${client} 即可使用。`,
            ],
          }
      : !loaded || input.justInstalled
        ? {
            words: [
              `New ${client} sessions load it when they start. In a session that's already open, run ${reloadCommand}.`,
              `新开的 ${client} 会话会自动加载。已经打开的会话，在其中运行 ${reloadCommand}。`,
            ],
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
export function mcpRow(input: McpInput): RowState {
  const { client } = input;
  const busy: Words | null =
    input.running === "add"
      ? ["Adding…", "正在添加…"]
      : input.running === "remove"
        ? ["Removing…", "正在移除…"]
        : null;
  const menu: RowState["menu"] = [];
  if (input.hasCommand)
    menu.push({ id: "copy", label: ["Copy command", "拷贝命令"] });
  if (input.added && input.setupReady)
    menu.push({
      id: "remove",
      label: [`Remove from ${client}`, `从 ${client} 移除`],
      destructive: true,
    });
  const copied = input.copied
    ? {
        words: [
          "Command copied. Paste it into Terminal.",
          "已拷贝命令，粘贴到终端运行。",
        ] as Words,
      }
    : null;
  if (input.setupReady === null && !busy)
    return {
      tone: "off",
      status: checking,
      action: null,
      busy: null,
      menu: [],
      next: null,
    };
  if (!input.added) {
    const unavailable: Words | undefined = !input.setupReady
      ? [
          "Adding HUD tools requires Agent HUD at its approved installation location and its MCP components.",
          "添加 HUD 工具需要指定安装位置的 Agent HUD 和完整的 MCP 组件。",
        ]
      : !input.clientFound
        ? [`Install ${client} first.`, `请先安装 ${client}。`]
        : undefined;
    return {
      tone: "off",
      status: input.setupReady
        ? ["Not added", "未添加"]
        : ["Unavailable", "不可用"],
      action: { id: "add", label: ["Add", "添加"], unavailable },
      busy,
      menu,
      next: copied,
    };
  }
  if (!input.toolsOn)
    return {
      tone: "attention",
      status: ["Tools off", "工具已关闭"],
      action: {
        id: "turn-on",
        label: ["Turn on", "打开"],
        unavailable: input.toolsUnavailable ?? undefined,
      },
      busy,
      menu,
      next: copied,
    };
  if (!input.listening)
    return {
      tone: "failed",
      status: ["Not running", "未运行"],
      action: null,
      busy,
      menu,
      next: copied,
    };
  return {
    tone: "working",
    status: ["Ready", "可以使用"],
    action: null,
    busy,
    menu,
    next:
      copied ??
      (input.justAdded
        ? {
            words: [
              `Start a new ${client} session and ask it to change the HUD.`,
              `在 ${client} 里新开一个会话，说“把 HUD 改成……”。`,
            ],
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
export function toolsSwitch(input: ToolsSwitchInput): {
  unavailable: Words | null;
  problem: Words | null;
} {
  const unavailable: Words | null = input.demo
    ? ["Not available in the demo.", "演示模式下不可用。"]
    : !input.supported
      ? [
          "This system can't serve HUD tools yet.",
          "这个系统暂时不能提供 HUD 工具。",
        ]
      : null;
  const problem: Words | null =
    input.supported && input.enabled && !input.listening
      ? [
          `Not running${input.notice ? `: ${input.notice}` : "."}`,
          `未运行${input.notice ? `：${input.notice}` : "。"}`,
        ]
      : !input.supported && !input.demo
        ? unavailable
        : null;
  return { unavailable, problem };
}
