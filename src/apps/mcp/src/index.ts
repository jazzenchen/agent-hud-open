import { McpServer, type RegisteredTool } from "@modelcontextprotocol/server";
import { serveStdio } from "@modelcontextprotocol/server/stdio";
import * as z from "zod/v4";
import type { Snapshot } from "@agent-hud/protocol";
import {
  askHost,
  caller,
  hostSchema,
  launchedBy,
  type HostTool,
} from "./host-tools.ts";
import { engine, guarded } from "./reply.ts";
import {
  checkedByTheApp,
  localWrite,
  proposal,
  readOnly,
  screen,
} from "./schemas.ts";

/** Asked at start, then again every `hostInterval` while the relay runs:
 * Antigravity and DeepSeek Harness start one relay for all their sessions,
 * so a group switched on later, or an app that was not running yet when the
 * relay started, must still reach their next sessions. Each change goes out
 * as notifications/tools/list_changed. An app that cannot be asked leaves
 * the list as it was. */
let hostTools = (await askHost()) ?? [];
const hostInterval =
  Number(process.env.AGENTHUD_HOST_TOOLS_MS) > 0
    ? Number(process.env.AGENTHUD_HOST_TOOLS_MS)
    : 15_000;

serveStdio(() => {
  const server = new McpServer({ name: "agent-hud", version: "0.1.0" });
  server.registerTool(
    "get_hud",
    {
      description:
        "Read the current hover panel as a tree of controls (panel) with the catalog of every control, data field and action it can hold (catalog), plus exact block/account/row keys, attached screens, the layout, resting HUD and app settings. Includes unsaved drafts. Call before editing; source keys remain valid when their readings temporarily disappear.",
      inputSchema: z.object({ screen }).strict(),
      annotations: readOnly,
    },
    (params) => guarded("get_hud", params),
  );
  server.registerTool(
    "draft_hud",
    {
      description:
        "Propose a change to the panel as edit steps on its tree (preferred), the whole tree as panel, a complete panel layout, or one screen's HUD style. A proposal is shown immediately and is saved only when the user presses Save in the panel; it is discarded on restart. Edit steps change only the nodes they name. Otherwise a proposal is whole: omitted layout fields use defaults, omitted HUD style fields keep saved values, and one that draws what is saved leaves no draft. A proposal the panel or the HUD cannot draw as asked is refused whole with every reason, and nothing is changed. from (default, previous or shared) proposes a version instead of a layout. app changes app settings only when explicitly requested by the user. Layout edits hold the panel open for two minutes; finish_hud_edit releases it.",
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
    () =>
      guarded("snapshot", {}, (snapshot: Snapshot) => ({
        platform: snapshot.connection.platform,
        platformCapabilities: snapshot.connection.platformCapabilities,
        capabilities: snapshot.capabilities,
      })),
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
              // Always, so the app can tell a client its hooks name (TRAE,
              // Kimi, ...) from Claude Code's session id inherited through
              // the environment by an agent started inside Claude Code.
              parent: launchedBy,
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
