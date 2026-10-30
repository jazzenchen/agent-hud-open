import {
  McpServer,
  type RegisteredTool,
  type StandardSchemaWithJSON,
} from "@modelcontextprotocol/server";
import { serveStdio } from "@modelcontextprotocol/server/stdio";
import * as z from "zod/v4";
import { EngineClient } from "@agent-hud/engine-client";
import type { Snapshot } from "@agent-hud/protocol";

const engine = new EngineClient(undefined, "app");
const readOnly = {
  readOnlyHint: true,
  destructiveHint: false,
  idempotentHint: true,
  openWorldHint: false,
};
const localWrite = {
  readOnlyHint: false,
  destructiveHint: false,
  idempotentHint: true,
  openWorldHint: false,
};
const result = (data: unknown) => {
  const object =
    typeof data === "object" && data !== null
      ? (data as Record<string, unknown>)
      : { value: data };
  const { images, ...fields } = object;
  const content: (
    | { type: "text"; text: string }
    | { type: "image"; data: string; mimeType: string }
  )[] = [{ type: "text", text: JSON.stringify(fields) }];
  if (Array.isArray(images))
    for (const image of images)
      if (
        image &&
        typeof image.data === "string" &&
        typeof image.mimeType === "string"
      )
        content.push({
          type: "image",
          data: image.data,
          mimeType: image.mimeType,
        });
  return { content, structuredContent: fields };
};
const guarded = async (
  method: string,
  params: Record<string, unknown> = {},
) => {
  try {
    return result(await engine.request(method, params));
  } catch (error) {
    return {
      isError: true,
      content: [
        {
          type: "text" as const,
          text:
            error instanceof Error ? error.message : "Engine operation failed.",
        },
      ],
    };
  }
};

/** Advertises `schema` to clients and leaves the checking to the app: it refuses a
 * proposal it cannot draw as asked with Agent HUD's own sentences, every reason at
 * once, and reads null as not given, as agent-hud-native-open's draft_hud does. zod would
 * refuse the same calls first, with generic messages. */
const checkedByTheApp = <S extends z.ZodType>(
  schema: S,
): StandardSchemaWithJSON<z.input<S>, z.output<S>> => ({
  "~standard": {
    ...schema["~standard"],
    validate: (value: unknown) =>
      typeof value === "object" && value !== null && !Array.isArray(value)
        ? { value: value as z.output<S> }
        : { issues: [{ message: "The arguments are a JSON object." }] },
  },
});
/** A tool the host app adds: its own group, which the app's settings switch on and off. */
type HostTool = {
  name: string;
  description: string;
  inputSchema: Record<string, unknown>;
  readOnly: boolean;
};
/** The host's groups that are on, or null when the app could not be asked. */
const askHost = () =>
  engine.request<{ tools?: HostTool[] }>("host_tools").then(
    (found) => found.tools ?? [],
    () => null,
  );
/** Asked at start, then again every `hostInterval` while the relay runs:
 * Antigravity and DeepSeek Harness start one relay for all their sessions,
 * so a group switched on later, or an app that was not running yet when the
 * relay started, must still reach their next sessions. Each change goes out
 * as notifications/tools/list_changed. An app that cannot be asked leaves
 * the list as it was. */
let hostTools = (await askHost()) ?? [];
const hostInterval =
  Number(process.env.AGENTHUD_NEXT_HOST_TOOLS_MS) > 0
    ? Number(process.env.AGENTHUD_NEXT_HOST_TOOLS_MS)
    : 15_000;
/** A host tool's schema as the host wrote it; the host checks the arguments itself. */
const hostSchema = (
  schema: Record<string, unknown>,
): StandardSchemaWithJSON<Record<string, unknown>> => ({
  "~standard": {
    version: 1,
    vendor: "agent-hud",
    validate: (value: unknown) =>
      typeof value === "object" && value !== null && !Array.isArray(value)
        ? { value: value as Record<string, unknown> }
        : { issues: [{ message: "The arguments are a JSON object." }] },
    jsonSchema: { input: () => schema, output: () => schema },
  },
});
/** The agent session that called, as its client names it and never as an argument the
 * agent writes: Codex and Antigravity in each call's `_meta`, Claude Code in the
 * environment it starts this relay with. DeepSeek Harness names neither: it runs one
 * relay per host for all its sessions. For a call no client names, the relay says which
 * process started it (`parent`), and the app finds the session from what Agent HUD's
 * plugin in that Harness host noted. */
const caller = (meta: Record<string, unknown> | undefined) => {
  const turn = meta?.["x-codex-turn-metadata"] as
    Record<string, unknown> | undefined;
  const thread = meta?.threadId ?? turn?.thread_id;
  if (typeof thread === "string" && thread)
    return {
      client: "codex",
      sessionId: thread,
      turnId: typeof turn?.turn_id === "string" ? turn.turn_id : null,
    };
  const cascade = meta?.["antigravity.google/conversation_id"];
  if (typeof cascade === "string" && cascade)
    return { client: "antigravity", sessionId: cascade, turnId: null };
  const session = process.env.CLAUDE_CODE_SESSION_ID;
  return session
    ? { client: "claude", sessionId: session, turnId: null }
    : null;
};
const keys = z.array(z.string().min(1));
const period = z.enum(["day", "week", "month"]);
const measures = z.array(
  z.enum(["quota", "burn", "tokens", "consumed", "cost"]),
);
const block = z
  .object({
    widget: z.enum(["bar", "ring", "number", "chart"]).optional(),
    measures: measures.optional(),
    hidden: z.boolean().optional(),
    hiddenRows: keys.optional(),
    hiddenAccounts: keys.optional(),
    rows: keys.optional(),
    accounts: keys.optional(),
    countdown: z.boolean().optional(),
    columns: z.number().int().min(2).max(6).optional(),
    ringSize: z.enum(["small", "medium", "large"]).optional(),
    span: z.enum(["full", "half"]).optional(),
    period: period.optional(),
  })
  .strict();
const screen = z
  .string()
  .min(1)
  .optional()
  .describe(
    "An attached screen key from get_hud, or all for the shared layout.",
  );
const hud = z
  .object({
    agents: keys.optional(),
    reset: z.boolean().optional(),
    mode: z.enum(["notch", "dynamic", "simple"]).optional(),
    edge: z.enum(["top", "right", "bottom", "left"]).optional(),
    position: z.number().min(0).max(1).optional(),
    logoSize: z.number().min(20).max(60).optional(),
    logoGap: z.number().min(0.1).max(0.6).optional(),
    edgeDistance: z.number().min(0).max(20).optional(),
    showLogos: z.boolean().optional(),
    simpleSize: z.number().min(20).max(60).optional(),
    simpleGap: z.number().min(0.1).max(0.6).optional(),
    ringWidth: z.number().min(1).max(4).optional(),
    showRings: z.boolean().optional(),
    showNumbers: z.boolean().optional(),
    separateAgents: z.boolean().optional(),
    appearance: z.enum(["dark", "light", "system"]).optional(),
    transparency: z.number().min(0).max(1).optional(),
    glow: z
      .object({
        style: z
          .enum(["blur", "dots", "ascii", "blocks", "braille", "binary"])
          .optional(),
        effect: z
          .enum(["breathe", "flow", "scan", "ripple", "shimmer", "boot"])
          .optional(),
        range: z.number().min(0).max(36).optional(),
        blur: z.number().min(0).max(36).optional(),
        brightness: z.number().min(0.2).max(1).optional(),
        speed: z.number().min(1).max(12).optional(),
        idleSpeed: z.number().min(1).max(24).optional(),
        amplitude: z.number().min(0).max(1).optional(),
        outwardOnly: z.boolean().optional(),
        gridPitch: z.number().min(4).max(12).optional(),
        gridDensity: z.number().min(0.5).max(1.5).optional(),
        gridCore: z.number().min(0).max(8).optional(),
        gridFade: z.number().min(0).max(8).optional(),
      })
      .strict()
      .optional(),
  })
  .strict();
const app = z
  .object({
    appearance: z.enum(["dark", "light", "system"]).optional(),
    matchScreen: z.boolean().optional(),
    botLogos: z.boolean().optional(),
    showMenuBarIcon: z.boolean().optional(),
    language: z
      .string()
      .optional()
      .describe("Read only; may repeat the value from get_hud."),
    writtenIn: z.string().optional().describe("Read only; may repeat get_hud."),
    hover: z
      .object({
        pressOptionToOpen: z.boolean().optional(),
        openAtTopEdge: z.boolean().optional(),
        delayMs: z.number().int().min(0).max(1500).multipleOf(50).optional(),
        collapseDelayMs: z
          .number()
          .int()
          .min(0)
          .max(1500)
          .multipleOf(50)
          .optional(),
      })
      .strict()
      .optional(),
  })
  .strict();
const proposal = z
  .object({
    screen,
    revision: z.number().int().nonnegative().optional(),
    from: z.enum(["default", "previous", "shared"]).optional(),
    order: keys.optional(),
    sections: z.array(z.enum(["blocks", "tokens", "sessions"])).optional(),
    hiddenSections: z
      .array(z.enum(["blocks", "tokens", "sessions"]))
      .optional(),
    countdown: z.boolean().optional(),
    blocks: z.record(z.string(), block).optional(),
    style: z
      .object({
        spacing: z.enum(["compact", "regular", "roomy"]).optional(),
        separators: z.boolean().optional(),
        width: z.number().int().min(480).max(720).optional(),
      })
      .strict()
      .optional(),
    sessions: z
      .object({
        limit: z.number().int().min(1).max(10).optional(),
        period: z.enum(["day", "week"]).optional(),
        agents: keys.optional(),
        projects: keys.optional(),
      })
      .strict()
      .optional(),
    tokens: z
      .object({
        period: period.optional(),
        agents: keys.optional(),
        models: keys.optional(),
      })
      .strict()
      .optional(),
    hud: hud.optional(),
    app: app.optional(),
  })
  .strict();
serveStdio(() => {
  const server = new McpServer({ name: "agent-hud", version: "0.1.0" });
  server.registerTool(
    "get_hud",
    {
      description:
        "Read the current hover panel, exact block/account/row keys, attached screens, layout, resting HUD and app settings. Includes unsaved drafts. Call before editing; source keys remain valid when their readings temporarily disappear.",
      inputSchema: z.object({ screen }).strict(),
      annotations: readOnly,
    },
    (params) => guarded("get_hud", params),
  );
  server.registerTool(
    "draft_hud",
    {
      description:
        "Propose a complete panel layout or one screen's HUD style. A proposal is shown immediately and is saved only when the user presses Save in the panel; it is discarded on restart. A proposal is whole: omitted layout fields use defaults, omitted HUD style fields keep saved values, and one that draws what is saved leaves no draft. A proposal the panel or the HUD cannot draw as asked is refused whole with every reason, and nothing is changed. from (default, previous or shared) proposes a version instead of a layout. app changes app settings only when explicitly requested by the user. Layout edits hold the panel open for two minutes; finish_hud_edit releases it.",
      inputSchema: checkedByTheApp(proposal),
      annotations: localWrite,
    },
    (params) => guarded("draft_hud", params as Record<string, unknown>),
  );
  server.registerTool(
    "discard_hud_draft",
    {
      description:
        "Discard a screen's layout and HUD style draft. The app-wide draft stays; the user's panel Discard drops all drafts it shows.",
      inputSchema: z.object({ screen }).strict(),
      annotations: localWrite,
    },
    (params) => guarded("discard_hud_draft", params),
  );
  server.registerTool(
    "finish_hud_edit",
    {
      description:
        "End the edit hold so panels collapse unless hovered. Drafts still await the user's Save; this does not save them.",
      inputSchema: z.object({ screen }).strict(),
      annotations: localWrite,
    },
    (params) => guarded("finish_hud_edit", params),
  );
  server.registerTool(
    "get_hud_screenshot",
    {
      description:
        "Render this app's hover panel, an agent's separate panel, or a screen's resting HUD. Accounts are masked and session titles omitted. Captures only the offscreen app WebView, never the desktop.",
      inputSchema: z
        .object({
          screen,
          show: z.enum(["panel", "hud"]).optional(),
          agent: z.string().min(1).optional(),
        })
        .strict(),
      annotations: readOnly,
    },
    (params) => guarded("get_hud_screenshot", params),
  );
  server.registerTool(
    "hud_snapshot",
    {
      description:
        "Read Agent HUD’s current local snapshot: recent/active display sessions, complete token totals, provider status and capabilities. Historical sessions are paged with hud_sessions. Unavailable data stays unavailable.",
      inputSchema: z.object({}),
      annotations: readOnly,
    },
    () => guarded("snapshot"),
  );
  server.registerTool(
    "hud_refresh",
    {
      description:
        "Read the latest snapshot already published by the running app. The app owns background refresh; this tool does not start provider reads or another collector.",
      inputSchema: z.object({}),
      annotations: readOnly,
    },
    () => guarded("refresh"),
  );
  server.registerTool(
    "hud_session",
    {
      description:
        "Read a known session’s recorded usage entries, parent and children by exact ID. Discover recent or historical sessions with hud_sessions; this query does not start ingestion.",
      inputSchema: z.object({
        id: z.string().min(1),
        messageCursor: z.number().int().nonnegative().optional(),
        messageLimit: z.number().int().min(1).max(200).optional(),
      }),
      annotations: readOnly,
    },
    (params) => guarded("session_detail", params),
  );
  server.registerTool(
    "hud_sessions",
    {
      description:
        "Page the running app’s already indexed sessions without refreshing providers or scanning transcripts. Recent is the display scope; active selects current work, all includes retained history. Filters use exact source/project IDs; search matches the recorded session presentation. Return the nextOffset to continue the same filters.",
      inputSchema: z.object({
        offset: z.number().int().min(0).default(0),
        limit: z.number().int().min(1).max(200).default(100),
        scope: z.enum(["recent", "active", "all"]).default("recent"),
        source: z.string().min(1).optional(),
        project: z
          .string()
          .nullable()
          .optional()
          .describe(
            "Exact project ID; null selects sessions without a project.",
          ),
        search: z.string().optional(),
        rootsOnly: z.boolean().default(true),
        parentId: z.string().min(1).optional(),
      }),
      annotations: readOnly,
    },
    (query) => guarded("sessions", query),
  );
  server.registerTool(
    "hud_statistics",
    {
      description:
        "Read interval usage, model/provider breakdowns, list-price estimates and local-calendar activity from the running app’s ledger. The app owns ingestion. Fifteen-minute source buckets count whole when they overlap the requested interval; coverage reports missing or still-indexing data.",
      inputSchema: z.object({
        from: z
          .string()
          .min(1)
          .describe("Interval start as an RFC3339 timestamp."),
        to: z
          .string()
          .min(1)
          .describe("Exclusive interval end as an RFC3339 timestamp."),
        bucketMinutes: z
          .union([z.literal(15), z.literal(30), z.literal(60), z.literal(1440)])
          .default(60),
        tokenKinds: z
          .array(
            z.enum(["cacheWrite", "input", "reasoning", "output", "cacheRead"]),
          )
          .min(1)
          .max(5)
          .optional(),
        providers: z
          .array(z.string().min(1))
          .optional()
          .describe(
            "Exact provider IDs; omitted selects all, an empty list selects none.",
          ),
        models: z
          .array(z.string().min(1))
          .optional()
          .describe(
            "Provider-qualified model IDs returned by this query; omitted selects all.",
          ),
        timeZone: z
          .string()
          .min(1)
          .describe(
            "IANA time zone, for example Asia/Singapore or America/New_York.",
          ),
      }),
      annotations: readOnly,
    },
    (query) => guarded("statistics", query),
  );
  server.registerTool(
    "hud_capabilities",
    {
      description:
        "Read the implemented and unimplemented product capabilities; this does not imply full migration parity.",
      inputSchema: z.object({}),
      annotations: readOnly,
    },
    async () => {
      try {
        const snapshot = await engine.request<Snapshot>("snapshot");
        return result({
          platform: snapshot.connection.platform,
          platformCapabilities: snapshot.connection.platformCapabilities,
          capabilities: snapshot.capabilities,
        });
      } catch (error) {
        return {
          isError: true,
          content: [
            {
              type: "text" as const,
              text:
                error instanceof Error ? error.message : "Engine unavailable.",
            },
          ],
        };
      }
    },
  );
  server.registerTool(
    "hud_set_preference",
    {
      description:
        "Save a user-requested presentation preference in Agent HUD. Does not change client configuration.",
      inputSchema: z.object({
        key: z.enum(["appearance", "animations"]),
        value: z.union([z.enum(["dark", "light", "system"]), z.boolean()]),
      }),
      annotations: localWrite,
    },
    ({ key, value }) => guarded("set_setting", { key, value }),
  );
  const hosted = new Map<
    string,
    { tool: HostTool; registered: RegisteredTool }
  >();
  /** Lists exactly `tools` of the host's, adding, replacing and removing as they changed. */
  const host = (tools: HostTool[]) => {
    const wanted = new Map(tools.map((tool) => [tool.name, tool]));
    for (const [name, entry] of hosted)
      if (JSON.stringify(wanted.get(name)) !== JSON.stringify(entry.tool)) {
        entry.registered.remove();
        hosted.delete(name);
      }
    for (const tool of tools) {
      if (hosted.has(tool.name)) continue;
      try {
        const registered = server.registerTool(
          tool.name,
          {
            description: tool.description,
            inputSchema: hostSchema(tool.inputSchema),
            annotations: tool.readOnly
              ? readOnly
              : { ...localWrite, idempotentHint: false },
          },
          (params, context) => {
            const named = caller(
              context.mcpReq._meta as Record<string, unknown> | undefined,
            );
            return guarded("host_tool", {
              name: tool.name,
              arguments: params,
              caller: named,
              ...(named ? {} : { parent: process.ppid }),
            });
          },
        );
        hosted.set(tool.name, { tool, registered });
      } catch {
        /* A name the relay's own tools already use stays theirs. */
      }
    }
  };
  host(hostTools);
  let asking = false;
  let closed = false;
  const watch = setInterval(async () => {
    if (asking) return;
    asking = true;
    const found = await askHost();
    asking = false;
    if (found && !closed) {
      hostTools = found;
      host(found);
    }
  }, hostInterval);
  watch.unref();
  server.server.onclose = () => {
    closed = true;
    clearInterval(watch);
    engine.close();
  };
  return server;
});

process.once("exit", () => engine.close());
process.once("SIGINT", () => {
  engine.close();
  process.exit(0);
});
process.once("SIGTERM", () => {
  engine.close();
  process.exit(0);
});
