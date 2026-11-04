// The host app's own tools, and the agent session that calls one.
import type { StandardSchemaWithJSON } from "@modelcontextprotocol/server";
import { engine } from "./reply.ts";
import { objectArguments } from "./schemas.ts";

/** A tool the host app adds: its own group, which the app's settings switch on and off. */
export type HostTool = {
  name: string;
  description: string;
  inputSchema: Record<string, unknown>;
  readOnly: boolean;
};
/** The host's groups that are on, or null when the app could not be asked. */
export const askHost = () =>
  engine.request<{ tools?: HostTool[] }>("host_tools").then(
    (found) => found.tools ?? [],
    () => null,
  );

/** A host tool's schema as the host wrote it; the host checks the arguments itself. */
export const hostSchema = (
  schema: Record<string, unknown>,
): StandardSchemaWithJSON<Record<string, unknown>> => ({
  "~standard": {
    version: 1,
    vendor: "agent-hud",
    validate: objectArguments<Record<string, unknown>>,
    jsonSchema: { input: () => schema, output: () => schema },
  },
});
/** The agent session that called, as its client names it and never as an argument the
 * agent writes: Codex and Antigravity in each call's `_meta`, Claude Code in the
 * environment it starts this relay with. DeepSeek Harness names neither: it runs one
 * relay per host for all its sessions. For a call no client names, the relay says which
 * process started it (`parent`), and the app finds the session from what Agent HUD's
 * plugin in that Harness host noted. On Windows the app's `--mcp` launcher stays as
 * this relay's parent, so it names the client in AGENTHUD_MCP_PARENT. */
export const launchedBy = (() => {
  const named = Number(process.env.AGENTHUD_MCP_PARENT);
  return Number.isSafeInteger(named) && named > 0 ? named : process.ppid;
})();
export const caller = (meta: Record<string, unknown> | undefined) => {
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
