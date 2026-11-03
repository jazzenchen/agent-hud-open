import { EngineClient } from "@agent-hud/engine-client";
import type { Plugin } from "vite";
const methods = new Set([
  "snapshot",
  "refresh",
  "statistics",
  "quota_history",
  "plan_value",
  "price_list",
  "sessions",
  "set_setting",
  "antigravity_cli_status",
  "set_provider",
  "answer_request",
  "session_detail",
  "open_session",
  "open_native",
  "toggle_hud",
]);
export function devBridge(): Plugin {
  const engine = new EngineClient();
  return {
    name: "agent-hud-dev-engine",
    configureServer(server) {
      server.httpServer?.once("close", () => engine.close());
      server.middlewares.use("/__bridge", async (request, response) => {
        response.setHeader("Content-Type", "application/json");
        response.setHeader("Cache-Control", "no-store");
        const origin = request.headers.origin;
        if (
          request.method !== "POST" ||
          (origin && origin !== `http://${request.headers.host}`)
        ) {
          response.statusCode = 403;
          response.end(
            JSON.stringify({
              error: {
                message: "Only same-origin local POST requests are accepted.",
              },
            }),
          );
          return;
        }
        try {
          let body = "";
          for await (const chunk of request) {
            body += chunk;
            if (body.length > 131072)
              throw new Error("Request exceeds the local command limit.");
          }
          const { method, params = {} } = JSON.parse(body);
          if (!methods.has(method)) throw new Error("Unknown engine command.");
          response.end(
            JSON.stringify({ result: await engine.request(method, params) }),
          );
        } catch (error) {
          response.statusCode = 503;
          response.end(
            JSON.stringify({
              error: {
                message: error instanceof Error ? error.message : String(error),
              },
            }),
          );
        }
      });
    },
  };
}
