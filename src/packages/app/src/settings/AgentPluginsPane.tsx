// Settings → Agents → Agent plugins: what Agent HUD puts into each agent client,
// one card per client. A row says what the thing lets you do, shows where it
// stands as a coloured chip, offers the one action that matters now, keeps
// removing in its ⋯ menu, and opens a next-step line only when there is a step.
// Claude Code's and DeepSeek Harness's cards lead with Agent HUD's plugin for
// that client, whose switches say what the plugin may do. A product build adds
// its own switches to that row, or rows of
// its own to a card, through `AgentPluginItem`, drawn with `PluginControl` and
// `PluginRow`, so both builds read the same.
import { Fragment, useEffect, useRef, useState, type ReactNode } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import {
  ArrowRight,
  Braces,
  Puzzle,
  Check,
  Copy,
  Info,
  LoaderCircle,
  MoreHorizontal,
  Wrench,
} from "lucide-react";
import type { NativeMethod, Snapshot } from "@agent-hud/protocol";
import { Button } from "@agent-hud/ui/components/button";
import { Switch } from "@agent-hud/ui/components/switch";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@agent-hud/ui/components/dropdown-menu";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@agent-hud/ui/components/tooltip";
import { HUDArtwork } from "@agent-hud/hud/HUDArtwork";
import { snapshotLanguage } from "@agent-hud/ui/format";
import {
  failure,
  mcpRow,
  pluginRow,
  toolsSwitch,
  type RowState,
  type Tone,
  type Words,
} from "../lib/agent-plugins";

/** What a product build's row is given to draw itself in a client's card. */
export interface AgentPluginContext {
  snapshot: Snapshot;
  /** The product's own requests (the "extension" method). */
  request: (method: string, params?: unknown) => Promise<unknown>;
  /** Whether the Settings window is on screen: poll only while it is. */
  active: boolean;
  /** Whether the client's command line is on this Mac; null while unknown. */
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
  /** "plugin": a switch in the client's Agent HUD plugin row (Claude Code's
   * or DeepSeek Harness's), after Next's own, drawn with `PluginControl`. "row" (the default): a row of its own,
   * drawn with `PluginRow`, before Next's rows. "tools": a tool group of the
   * product's own in the tools section, after the HUD tools switch, drawn
   * with `ToolSwitchRow`; `client` is not read. */
  place?: "row" | "plugin" | "tools";
  /** The card's name when Next has no card for this client. */
  clientName?: string;
  render(context: AgentPluginContext): ReactNode;
}

/** native_mcp_clients: which clients have Agent HUD's MCP server, and the
 * command and settings to add it by hand. */
export interface MCPClientsStatus {
  ready: boolean;
  notice: string | null;
  json: unknown;
  clients: {
    id: string;
    name: string;
    available: boolean;
    installed: boolean;
    /** The client's own command that adds the server; null when Agent HUD edits its settings file. */
    command: string | null;
  }[];
}
export type MCPClientsService = (
  action: "status" | "install" | "remove",
  client?: string,
) => Promise<MCPClientsStatus>;

const nativeClients: MCPClientsService = async (action, client) => {
  if (!isTauri())
    return { ready: false, notice: null, json: null, clients: [] };
  return invoke<MCPClientsStatus>("native_mcp_clients", {
    action,
    client: client ?? null,
  });
};

/** native_claude_plugin: whether Claude Code is here, whether it lists Agent
 * HUD's plugin, and how many sessions run it now. */
export interface ClaudePluginStatus {
  found: boolean;
  installed: boolean;
  sessions: number;
}
export type ClaudePluginService = (
  action: "status" | "install" | "remove",
) => Promise<ClaudePluginStatus>;

const nativePlugin: ClaudePluginService = async (action) => {
  if (!isTauri()) return { found: false, installed: false, sessions: 0 };
  return invoke<ClaudePluginStatus>("native_claude_plugin", { action });
};

/** native_deepseek_plugin: whether DeepSeek Harness is here, whether its home
 * patch loads Agent HUD's plugin, and how many sessions run it now. */
const nativeDeepSeekPlugin: ClaudePluginService = async (action) => {
  if (!isTauri()) return { found: false, installed: false, sessions: 0 };
  return invoke<ClaudePluginStatus>("native_deepseek_plugin", { action });
};

/** The clients the host can give HUD tools to (hud_tools::CLIENTS). */
const knownClients = [
  { id: "claudeCode", name: "Claude Code", logo: "claude" },
  { id: "codex", name: "Codex", logo: "codex" },
  { id: "cursor", name: "Cursor", logo: "cursor" },
  { id: "deepseek", name: "DeepSeek Harness", logo: "deepseek" },
  { id: "antigravity", name: "Antigravity", logo: "antigravity" },
];

let cachedStatus: MCPClientsStatus | null = null;
/** Each client plugin's last status, so the page opens without a blank. */
const cachedPlugins: Record<string, ClaudePluginStatus | null> = {};

function say(zh: boolean, words: Words) {
  return zh ? words[1] : words[0];
}

export function StatusChip({
  tone,
  busy = false,
  label,
}: {
  tone: Tone;
  busy?: boolean;
  label: string;
}) {
  return (
    <span
      className="agent-plugins-chip"
      data-tone={tone}
      data-busy={busy || undefined}
      role="status"
    >
      {busy ? (
        <LoaderCircle
          className="agent-plugins-spinner"
          size={11}
          strokeWidth={2.4}
          aria-hidden
        />
      ) : (
        <span className="agent-plugins-dot" aria-hidden />
      )}
      <span key={label} className="agent-plugins-chip-label">
        {label}
      </span>
    </span>
  );
}

/** Keeps the last content while it closes, so the line folds instead of vanishing. */
function Reveal({ open, children }: { open: boolean; children: ReactNode }) {
  const last = useRef<ReactNode>(null);
  if (open) last.current = children;
  return (
    <div
      className="agent-plugins-reveal"
      data-open={open}
      aria-hidden={!open}
      inert={!open}
    >
      <div>{open ? children : last.current}</div>
    </div>
  );
}

function CopyButton({ text, zh }: { text: string; zh: boolean }) {
  const [copied, setCopied] = useState(false);
  useEffect(() => {
    if (!copied) return;
    const timer = setTimeout(() => setCopied(false), 1600);
    return () => clearTimeout(timer);
  }, [copied]);
  return (
    <button
      type="button"
      className="agent-plugins-copy"
      aria-label={
        copied ? say(zh, ["Copied", "已拷贝"]) : say(zh, ["Copy", "拷贝"])
      }
      onClick={() =>
        void navigator.clipboard
          ?.writeText(text)
          .then(() => setCopied(true))
          .catch(() => {})
      }
    >
      {copied ? <Check size={12} /> : <Copy size={12} />}
    </button>
  );
}

function NextStep({
  words,
  command,
  zh,
}: {
  words: Words;
  command?: string;
  zh: boolean;
}) {
  const text = say(zh, words);
  const at = command ? text.indexOf(command) : -1;
  return (
    <div className="agent-plugins-next">
      <ArrowRight size={13} aria-hidden />
      <span>
        {at < 0 ? (
          text
        ) : (
          <>
            {text.slice(0, at)}
            <code>{command}</code>
            <CopyButton text={command!} zh={zh} />
            {text.slice(at + command!.length)}
          </>
        )}
      </span>
    </div>
  );
}

function Disabled({
  reason,
  children,
}: {
  reason: string;
  children: ReactNode;
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span tabIndex={0} style={{ display: "inline-flex" }}>
          {children}
        </span>
      </TooltipTrigger>
      <TooltipContent style={{ maxWidth: 260 }}>{reason}</TooltipContent>
    </Tooltip>
  );
}

/** One thing Agent HUD adds to a client: glyph, what it does, its chip, its one
 * action, its ⋯ menu and, when there is one, its next step or failure. */
export function PluginRow({
  icon,
  title,
  tag,
  description,
  note,
  state,
  error,
  zh,
  onAction,
  children,
  controlsOpen = true,
}: {
  icon: ReactNode;
  title: string;
  tag?: string;
  description: string;
  /** A further line under the description, such as what an extension adds. */
  note?: string | null;
  state: RowState;
  error?: string | null;
  zh: boolean;
  onAction: (id: string) => void;
  /** Switches for what the thing may do, drawn with `PluginControl`. */
  children?: ReactNode;
  /** Whether the switches show; they fold open once there is something to switch. */
  controlsOpen?: boolean;
}) {
  const busy = state.busy !== null;
  const action = state.action;
  const button = action && (
    <Button
      variant="outline"
      size="sm"
      className="agent-plugins-action"
      disabled={busy || Boolean(action.unavailable)}
      onClick={() => onAction(action.id)}
    >
      {say(zh, action.label)}
    </Button>
  );
  return (
    <div className="agent-plugins-row" data-tone={state.tone}>
      <span className="agent-plugins-glyph" aria-hidden>
        {icon}
      </span>
      <div className="agent-plugins-text">
        <div className="agent-plugins-title">
          {title}
          {tag && <span className="agent-plugins-tag">{tag}</span>}
        </div>
        <p className="agent-plugins-description">{description}</p>
        {note && <p className="agent-plugins-note">{note}</p>}
      </div>
      <div className="agent-plugins-side">
        <StatusChip
          tone={busy ? "off" : state.tone}
          busy={busy}
          label={say(zh, state.busy ?? state.status)}
        />
        {button &&
          (action?.unavailable && !busy ? (
            <Disabled reason={say(zh, action.unavailable)}>{button}</Disabled>
          ) : (
            button
          ))}
        {state.menu.length ? (
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                variant="ghost"
                size="icon-xs"
                className="agent-plugins-more"
                disabled={busy}
                aria-label={say(zh, ["More", "更多"])}
              >
                <MoreHorizontal size={15} />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              {state.menu.map((item) => (
                <DropdownMenuItem
                  key={item.id}
                  variant={item.destructive ? "destructive" : "default"}
                  onClick={() => onAction(item.id)}
                >
                  {say(zh, item.label)}
                </DropdownMenuItem>
              ))}
            </DropdownMenuContent>
          </DropdownMenu>
        ) : (
          <span className="agent-plugins-more-space" aria-hidden />
        )}
      </div>
      {children && (
        <Reveal open={controlsOpen}>
          <div className="agent-plugins-controls">{children}</div>
        </Reveal>
      )}
      <Reveal open={Boolean(state.next || error)}>
        {state.next && (
          <NextStep
            words={state.next.words}
            command={state.next.command}
            zh={zh}
          />
        )}
        {error && (
          <p className="agent-plugins-error" role="alert">
            {error}
          </p>
        )}
      </Reveal>
    </div>
  );
}

/** One thing a plugin may do, with its switch, inside the plugin's row. */
export function PluginControl({
  title,
  description,
  checked,
  onChange,
  unavailable,
  pending = false,
}: {
  title: string;
  description: string;
  checked: boolean;
  onChange: (value: boolean) => void;
  /** Why it cannot change now; the switch is drawn disabled with this as its tip. */
  unavailable?: string | null;
  pending?: boolean;
}) {
  const control = (
    <Switch
      aria-label={title}
      checked={checked}
      disabled={Boolean(unavailable) || pending}
      onCheckedChange={onChange}
    />
  );
  return (
    <div
      className="agent-plugins-control"
      data-unavailable={Boolean(unavailable) || undefined}
    >
      <div className="agent-plugins-text">
        <div className="agent-plugins-control-title">{title}</div>
        <p className="agent-plugins-description">{description}</p>
      </div>
      {unavailable ? (
        <Disabled reason={unavailable}>{control}</Disabled>
      ) : (
        control
      )}
    </div>
  );
}

/** A tool group's switch in the tools section: what agents may do with its
 * tools and, while it is on, the group's own settings, drawn with
 * `PluginControl`. Next's is HUD tools'; a product build adds its groups'. */
export function ToolSwitchRow({
  title,
  description,
  checked,
  onChange,
  pending = false,
  unavailable,
  error,
  children,
}: {
  title: string;
  description: string;
  checked: boolean;
  onChange: (value: boolean) => void;
  pending?: boolean;
  /** Why it cannot change now; the switch is drawn disabled with this as its tip. */
  unavailable?: string | null;
  error?: string | null;
  children?: ReactNode;
}) {
  const control = (
    <Switch
      aria-label={title}
      checked={checked}
      disabled={Boolean(unavailable) || pending}
      onCheckedChange={onChange}
    />
  );
  return (
    <div className="agent-plugins-row agent-plugins-switch-row">
      <div className="agent-plugins-text">
        <div className="agent-plugins-title">{title}</div>
        <p className="agent-plugins-description">{description}</p>
      </div>
      <div className="agent-plugins-side">
        {unavailable ? (
          <Disabled reason={unavailable}>{control}</Disabled>
        ) : (
          control
        )}
      </div>
      {children && (
        <Reveal open={checked}>
          <div className="agent-plugins-controls">{children}</div>
        </Reveal>
      )}
      <Reveal open={Boolean(error)}>
        <p className="agent-plugins-error" role="alert">
          {error}
        </p>
      </Reveal>
    </div>
  );
}

/** A client plugin's status, read again every 10 seconds while Settings is on
 * screen (the loaded count follows sessions checking in) and when the app
 * comes to the front, and its install and remove. */
function usePlugin(
  client: string,
  service: ClaudePluginService,
  active: boolean,
  zh: boolean,
  onStatus?: (status: ClaudePluginStatus) => void,
) {
  const [status, setStatus] = useState<ClaudePluginStatus | null>(
    cachedPlugins[client] ?? null,
  );
  const [running, setRunning] = useState<"install" | "remove" | null>(null);
  const [justInstalled, setJustInstalled] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const read = useRef(service);
  read.current = service;
  const report = useRef(onStatus);
  report.current = onStatus;
  const apply = (next: ClaudePluginStatus) => {
    cachedPlugins[client] = next;
    setStatus(next);
    report.current?.(next);
  };
  useEffect(() => {
    if (!active) return;
    let dead = false;
    const again = () =>
      void read
        .current("status")
        .then((next) => {
          if (!dead) apply(next);
        })
        .catch((failed) => {
          if (!dead) setError(String(failed));
        });
    again();
    const timer = setInterval(again, 10_000);
    window.addEventListener("focus", again);
    return () => {
      dead = true;
      clearInterval(timer);
      window.removeEventListener("focus", again);
    };
  }, [active]);
  async function change(kind: "install" | "remove") {
    setRunning(kind);
    setError(null);
    try {
      apply(await read.current(kind));
      setJustInstalled(kind === "install");
    } catch (failed) {
      setError(say(zh, failure(kind, String(failed))));
    } finally {
      setRunning(null);
    }
  }
  return { status, running, justInstalled, error, change };
}

/** Agent HUD's Claude Code plugin: one install, then a switch for each thing
 * it may do. Next's own is answering Claude's questions on the HUD; a
 * product build adds its switches after it. */
function ClaudePluginRow({
  context,
  extra,
  service,
  action,
  onStatus,
}: {
  context: Omit<AgentPluginContext, "plugin">;
  extra: AgentPluginItem[];
  service: ClaudePluginService;
  action: (
    method: NativeMethod,
    params?: Record<string, unknown>,
  ) => Promise<unknown>;
  onStatus: (status: ClaudePluginStatus) => void;
}) {
  const { snapshot, active } = context;
  const zh = snapshotLanguage(snapshot) === "zh";
  const t = (words: Words) => say(zh, words);
  const { status, running, justInstalled, error, change } = usePlugin(
    "claudeCode",
    service,
    active,
    zh,
    onStatus,
  );
  const [answering, setAnswering] = useState(false);
  const installed = status ? status.installed : null;
  const state = pluginRow({
    installed,
    sessions: status?.sessions ?? 0,
    clientFound: status ? status.found : context.clientFound,
    running,
    justInstalled,
  });
  const answers = snapshot.settings.claudeAnswers !== false;
  return (
    <PluginRow
      icon={<Puzzle size={14} strokeWidth={1.9} />}
      title={t(["Agent HUD plugin", "Agent HUD 插件"])}
      description={t([
        "Agent HUD's plugin for Claude Code. Install it once, then choose what it may do.",
        "Agent HUD 的 Claude Code 插件。安装一次，再选择它可以做什么。",
      ])}
      state={state}
      error={error}
      zh={zh}
      controlsOpen={installed === true}
      onAction={(id) => {
        if (id === "install" || id === "remove") void change(id);
      }}
    >
      <PluginControl
        title={t(["Answer questions", "回答提问"])}
        description={t([
          "Answer Claude's questions from the HUD. Your answer goes to the session that asked.",
          "在 HUD 上回答 Claude 的提问，回答会发回提问的会话。",
        ])}
        checked={answers}
        pending={answering}
        onChange={(value) => {
          setAnswering(true);
          void action("set_setting", { key: "claudeAnswers", value })
            .catch(() => {})
            .finally(() => setAnswering(false));
        }}
      />
      {extra.map((item) => (
        <Fragment key={item.id}>
          {item.render({ ...context, plugin: { installed } })}
        </Fragment>
      ))}
    </PluginRow>
  );
}

/** Agent HUD's DeepSeek Harness plugin: one install, which every Harness
 * host loads by itself, the Desktop app's included. It shows the questions
 * and approvals a session waits on in the HUD, beside Harness's own cards;
 * a product build adds its switches to the row. */
function DeepSeekPluginRow({
  context,
  extra,
  service,
  onStatus,
}: {
  context: Omit<AgentPluginContext, "plugin">;
  extra: AgentPluginItem[];
  service: ClaudePluginService;
  onStatus: (status: ClaudePluginStatus) => void;
}) {
  const { snapshot, active } = context;
  const zh = snapshotLanguage(snapshot) === "zh";
  const t = (words: Words) => say(zh, words);
  const { status, running, justInstalled, error, change } = usePlugin(
    "deepseek",
    service,
    active,
    zh,
    onStatus,
  );
  const installed = status ? status.installed : null;
  const state = pluginRow(
    {
      installed,
      sessions: status?.sessions ?? 0,
      clientFound: status ? status.found : context.clientFound,
      running,
      justInstalled,
    },
    "DeepSeek Harness",
    true,
  );
  return (
    <PluginRow
      icon={<Puzzle size={14} strokeWidth={1.9} />}
      title={t(["Agent HUD plugin", "Agent HUD 插件"])}
      description={t([
        "Answer DeepSeek Harness's questions and approvals from the HUD. Whichever you answer first, the HUD or Harness, counts.",
        "在 HUD 上回答 DeepSeek Harness 的提问和批准请求。HUD 和 Harness 里先回答的那个算数。",
      ])}
      state={state}
      error={error}
      zh={zh}
      controlsOpen={installed === true && extra.length > 0}
      onAction={(id) => {
        if (id === "install" || id === "remove") void change(id);
      }}
    >
      {extra.length > 0 &&
        extra.map((item) => (
          <Fragment key={item.id}>
            {item.render({ ...context, plugin: { installed } })}
          </Fragment>
        ))}
    </PluginRow>
  );
}

function ClientCard({
  name,
  logo,
  missing,
  light,
  children,
}: {
  name: string;
  logo?: string;
  missing?: string | null;
  light: boolean;
  children: ReactNode;
}) {
  return (
    <section className="agent-plugins-client">
      <header>
        {logo && <HUDArtwork source={logo} size={18} light={light} />}
        <h2>{name}</h2>
        {missing && <span className="agent-plugins-missing">{missing}</span>}
      </header>
      <div className="agent-plugins-card">{children}</div>
    </section>
  );
}

export function AgentPluginsPane({
  snapshot,
  action,
  active = true,
  items = [],
  request,
  clients = nativeClients,
  plugin = nativePlugin,
  deepseekPlugin = nativeDeepSeekPlugin,
}: {
  snapshot: Snapshot;
  action: (
    method: NativeMethod,
    params?: Record<string, unknown>,
  ) => Promise<unknown>;
  active?: boolean;
  items?: AgentPluginItem[];
  request: AgentPluginContext["request"];
  /** The MCP setup service; a fake in pictures and tests. */
  clients?: MCPClientsService;
  /** The Claude Code plugin's service; a fake in pictures and tests. */
  plugin?: ClaudePluginService;
  /** The DeepSeek Harness plugin's service; a fake in pictures and tests. */
  deepseekPlugin?: ClaudePluginService;
}) {
  const zh = snapshotLanguage(snapshot) === "zh";
  const t = (words: Words) => say(zh, words);
  const light =
    typeof document !== "undefined" &&
    document.documentElement.dataset.theme === "light";
  const [status, setStatus] = useState<MCPClientsStatus | null>(cachedStatus);
  const [running, setRunning] = useState<Record<string, "add" | "remove">>({});
  const [justAdded, setJustAdded] = useState<string[]>([]);
  const [copied, setCopied] = useState<string | null>(null);
  const [copiedSettings, setCopiedSettings] = useState(false);
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [toolsPending, setToolsPending] = useState(false);
  const [toolsError, setToolsError] = useState<string | null>(null);
  const [pluginInstalled, setPluginInstalled] = useState<
    Record<string, boolean | null>
  >(() =>
    Object.fromEntries(
      Object.entries(cachedPlugins).map(([id, status]) => [
        id,
        status?.installed ?? null,
      ]),
    ),
  );
  const pluginReport = (client: string) => (next: ClaudePluginStatus) =>
    setPluginInstalled((previous) =>
      previous[client] === next.installed
        ? previous
        : { ...previous, [client]: next.installed },
    );
  const service = useRef(clients);
  service.current = clients;
  const apply = (next: MCPClientsStatus) => {
    cachedStatus = next;
    setStatus(next);
  };
  // Read when the page opens, when the window is back on screen, and when the
  // app comes to the front: a command run in Terminal shows up (as Open does).
  useEffect(() => {
    if (!active) return;
    let dead = false;
    const read = () =>
      void service
        .current("status")
        .then((next) => {
          if (!dead) apply(next);
        })
        .catch((error) => {
          if (!dead)
            apply({
              ready: false,
              notice: String(error),
              json: null,
              clients: [],
            });
        });
    read();
    window.addEventListener("focus", read);
    return () => {
      dead = true;
      window.removeEventListener("focus", read);
    };
  }, [active]);
  useEffect(() => {
    if (!copied) return;
    const timer = setTimeout(() => setCopied(null), 4000);
    return () => clearTimeout(timer);
  }, [copied]);
  useEffect(() => {
    if (!copiedSettings) return;
    const timer = setTimeout(() => setCopiedSettings(false), 2000);
    return () => clearTimeout(timer);
  }, [copiedSettings]);

  const tools = snapshot.connection.localTools;
  const toolsOn = Boolean(snapshot.settings.hudTools);
  const demo = snapshot.mode === "demo";
  const toolState = toolsSwitch({
    supported: Boolean(tools?.supported),
    enabled: toolsOn,
    listening: Boolean(tools?.listening),
    notice: tools?.notice ?? null,
    demo,
  });
  async function setTools(value: boolean) {
    setToolsPending(true);
    setToolsError(null);
    try {
      await action("set_setting", { key: "hudTools", value });
    } catch (error) {
      setToolsError(String(error));
    } finally {
      setToolsPending(false);
    }
  }
  async function change(client: string, kind: "add" | "remove") {
    setRunning((previous) => ({ ...previous, [client]: kind }));
    setErrors(({ [client]: _, ...rest }) => rest);
    setCopied(null);
    try {
      apply(
        await service.current(kind === "add" ? "install" : "remove", client),
      );
      setJustAdded((previous) =>
        kind === "add"
          ? [...previous, client]
          : previous.filter((id) => id !== client),
      );
    } catch (error) {
      setErrors((previous) => ({
        ...previous,
        [client]: t(failure(kind, String(error))),
      }));
    } finally {
      setRunning(({ [client]: _, ...rest }) => rest);
    }
  }

  const ready = status?.ready ?? null;
  const groups = items.filter((item) => item.place === "tools");
  const cards = [
    ...knownClients,
    ...items
      .filter((item) => item.place !== "tools")
      .filter(
        (item) => !knownClients.some((client) => client.id === item.client),
      )
      .filter(
        (item, index, all) =>
          all.findIndex((other) => other.client === item.client) === index,
      )
      .map((item) => ({
        id: item.client,
        name: item.clientName ?? item.client,
        logo: undefined as string | undefined,
      })),
  ];
  return (
    <TooltipProvider delayDuration={300}>
      <div className="agent-plugins">
        {ready === false && (
          <p className="agent-plugins-notice">
            <Info size={13} aria-hidden />
            {t([
              "HUD tools can only be added from the Agent HUD installed in Applications.",
              "只有装在「应用程序」里的 Agent HUD 才能添加 HUD 工具。",
            ])}
          </p>
        )}
        {cards.map((card) => {
          const entry = status?.clients.find((client) => client.id === card.id);
          const known = knownClients.some((client) => client.id === card.id);
          const found = entry ? entry.available : null;
          const own = items.filter(
            (item) =>
              item.client === card.id && (item.place ?? "row") === "row",
          );
          const context = { snapshot, request, active, clientFound: found };
          const row =
            known &&
            mcpRow({
              client: card.name,
              setupReady: status ? status.ready : null,
              clientFound: found !== false,
              added: Boolean(entry?.installed),
              hasCommand: Boolean(entry?.command),
              toolsOn,
              toolsUnavailable: toolState.unavailable,
              listening: Boolean(tools?.listening),
              running: running[card.id] ?? null,
              justAdded: justAdded.includes(card.id),
              copied: copied === card.id,
            });
          return (
            <ClientCard
              key={card.id}
              name={card.name}
              logo={card.logo}
              light={light}
              missing={
                entry && !entry.available
                  ? t(["Not found on this Mac", "这台 Mac 上没有找到"])
                  : null
              }
            >
              {card.id === "claudeCode" && (
                <ClaudePluginRow
                  context={context}
                  extra={items.filter(
                    (item) =>
                      item.client === card.id && item.place === "plugin",
                  )}
                  service={plugin}
                  action={action}
                  onStatus={pluginReport(card.id)}
                />
              )}
              {card.id === "deepseek" && (
                <DeepSeekPluginRow
                  context={context}
                  extra={items.filter(
                    (item) =>
                      item.client === card.id && item.place === "plugin",
                  )}
                  service={deepseekPlugin}
                  onStatus={pluginReport(card.id)}
                />
              )}
              {own.map((item) => (
                <Fragment key={item.id}>
                  {item.render({
                    ...context,
                    plugin: { installed: pluginInstalled[card.id] ?? null },
                  })}
                </Fragment>
              ))}
              {row && (
                <PluginRow
                  icon={<Wrench size={14} strokeWidth={1.9} />}
                  title={t(["HUD tools", "HUD 工具"])}
                  tag="MCP"
                  description={t([
                    `${card.name} can read the HUD and propose layouts and styles for you to save.`,
                    `${card.name} 可以读取 HUD，提出布局和样式草稿，由你保存。`,
                  ])}
                  state={row}
                  error={errors[card.id]}
                  zh={zh}
                  onAction={(id) => {
                    if (id === "add" || id === "remove")
                      void change(card.id, id);
                    else if (id === "turn-on") void setTools(true);
                    else if (id === "copy" && entry?.command)
                      void navigator.clipboard
                        ?.writeText(entry.command)
                        .then(() => setCopied(card.id))
                        .catch(() => {});
                  }}
                />
              )}
            </ClientCard>
          );
        })}
        <ClientCard name={t(["Other agents", "其他智能体"])} light={light}>
          <div className="agent-plugins-row">
            <span className="agent-plugins-glyph" aria-hidden>
              <Braces size={14} strokeWidth={1.9} />
            </span>
            <div className="agent-plugins-text">
              <div className="agent-plugins-title">
                {t(["HUD tools", "HUD 工具"])}
                <span className="agent-plugins-tag">MCP</span>
              </div>
              <p className="agent-plugins-description">
                {t([
                  "Add them by hand: paste these settings into the agent's MCP configuration.",
                  "手动添加：把这段配置粘贴到智能体的 MCP 设置里。",
                ])}
              </p>
            </div>
            <div className="agent-plugins-side">
              <Button
                variant="outline"
                size="sm"
                className="agent-plugins-action"
                disabled={status?.json == null}
                onClick={() =>
                  void navigator.clipboard
                    ?.writeText(JSON.stringify(status?.json, null, 2))
                    .then(() => setCopiedSettings(true))
                    .catch(() => {})
                }
              >
                {copiedSettings ? (
                  <>
                    <Check size={13} aria-hidden />
                    {t(["Copied", "已拷贝"])}
                  </>
                ) : (
                  t(["Copy settings", "拷贝配置"])
                )}
              </Button>
              <span className="agent-plugins-more-space" aria-hidden />
            </div>
          </div>
        </ClientCard>
        <section className="agent-plugins-client">
          <header>
            <h2>
              {groups.length
                ? t(["Agent tools", "智能体工具"])
                : t(["HUD tools", "HUD 工具"])}
            </h2>
          </header>
          <div className="agent-plugins-card">
            <ToolSwitchRow
              title={t(["Let agents arrange the HUD", "允许智能体调整 HUD"])}
              description={t([
                "Agents with HUD tools can rearrange the panel and restyle the HUD. A change stays a draft until you press Save in the panel.",
                "接入了 HUD 工具的智能体可以帮你重排面板、改 HUD 样式。改动先是草稿，你在面板里点“保存”才生效。",
              ])}
              checked={toolsOn}
              pending={toolsPending}
              unavailable={
                toolState.unavailable ? t(toolState.unavailable) : null
              }
              error={
                toolsError ?? (toolState.problem ? t(toolState.problem) : null)
              }
              onChange={(value) => void setTools(value)}
            />
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
          </div>
        </section>
      </div>
    </TooltipProvider>
  );
}
