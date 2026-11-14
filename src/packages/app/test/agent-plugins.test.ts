import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { addCatalog, translator } from "@agent-hud/ui/format";
import {
  mcpRow,
  pluginRow,
  toolsSwitch,
  type McpInput,
} from "../src/lib/agent-plugins.ts";

// The counted forms ("1 session") are English's, in en.json.
addCatalog(
  "en",
  JSON.parse(
    readFileSync(
      join(import.meta.dirname, "../../protocol/locales/en.json"),
      "utf8",
    ),
  ),
);
const t = translator("en");

const plugin = {
  installed: false,
  sessions: 0,
  clientFound: true,
  running: null,
  justInstalled: false,
} as const;
const mcp: McpInput = {
  client: "Codex",
  setupReady: true,
  clientFound: true,
  added: false,
  hasCommand: true,
  toolsOn: false,
  toolsUnavailable: null,
  listening: false,
  running: null,
  justAdded: false,
  copied: false,
};

test("the plugin row offers Install until it is in, then says how many sessions run it", () => {
  const reading = pluginRow(t, { ...plugin, installed: null });
  assert.deepEqual([reading.status, reading.action], ["Checking…", null]);

  const missing = pluginRow(t, plugin);
  assert.deepEqual(
    [
      missing.tone,
      missing.status,
      missing.action?.id,
      missing.action?.unavailable,
    ],
    ["off", "Not installed", "install", undefined],
  );
  assert.deepEqual(missing.menu, []);

  const noClient = pluginRow(t, { ...plugin, clientFound: false });
  assert.equal(noClient.action?.unavailable, "Install Claude Code first.");

  const installing = pluginRow(t, { ...plugin, running: "install" });
  assert.equal(installing.busy, "Installing…");

  // In place but no session has loaded it: blue, no button, and the step that loads it.
  const idle = pluginRow(t, { ...plugin, installed: true });
  assert.deepEqual(
    [idle.tone, idle.status, idle.action],
    ["installed", "Installed", null],
  );
  assert.equal(idle.next?.command, "/reload-plugins");
  assert.match(idle.next!.words, /In an open session, run \/reload-plugins/);
  assert.deepEqual(
    idle.menu.map((item) => [item.id, item.destructive]),
    [["remove", true]],
  );

  const loaded = pluginRow(t, { ...plugin, installed: true, sessions: 2 });
  assert.deepEqual(
    [loaded.tone, loaded.status],
    ["working", "Loaded in 2 sessions"],
  );
  assert.equal(loaded.next, null);
  assert.equal(
    pluginRow(t, { ...plugin, installed: true, sessions: 1 }).status,
    "Loaded in 1 session",
  );
  // Right after installing, the reload step stays even once a session loaded it.
  assert.ok(
    pluginRow(t, {
      ...plugin,
      installed: true,
      sessions: 1,
      justInstalled: true,
    }).next,
  );

  assert.deepEqual(
    pluginRow(t, { ...plugin, installed: true, sessions: 2, running: "remove" })
      .busy,
    "Removing…",
  );
});

test("a plugin the client loads by itself only asks to open the client, never to reload", () => {
  const client = "DeepSeek Harness";
  assert.equal(
    pluginRow(t, { ...plugin, clientFound: false }, client, true).action
      ?.unavailable,
    "Install DeepSeek Harness first.",
  );
  const idle = pluginRow(t, { ...plugin, installed: true }, client, true);
  assert.equal(idle.status, "Installed");
  assert.equal(idle.next?.command, undefined);
  assert.match(idle.next?.words ?? "", /^DeepSeek Harness loads it by itself/);
  assert.match(idle.next?.words ?? "", /Open DeepSeek Harness to use it/);
  const loaded = pluginRow(
    t,
    { ...plugin, installed: true, sessions: 2, justInstalled: true },
    client,
    true,
  );
  assert.deepEqual([loaded.tone, loaded.next], ["working", null]);
  assert.equal(loaded.menu[0]?.id, "remove");
});

test("the MCP row asks for the one step left, and never removes from its main button", () => {
  assert.equal(mcpRow(t, { ...mcp, setupReady: null }).status, "Checking…");

  const notAdded = mcpRow(t, mcp);
  assert.deepEqual(
    [
      notAdded.tone,
      notAdded.status,
      notAdded.action?.id,
      notAdded.action?.unavailable,
    ],
    ["off", "Not added", "add", undefined],
  );
  assert.deepEqual(
    notAdded.menu.map((item) => item.id),
    ["copy"],
  );

  const noClient = mcpRow(t, { ...mcp, clientFound: false });
  assert.equal(noClient.action?.unavailable, "Install Codex first.");

  const notInstalledApp = mcpRow(t, {
    ...mcp,
    setupReady: false,
    hasCommand: false,
  });
  assert.deepEqual(
    [
      notInstalledApp.status,
      notInstalledApp.action?.unavailable,
      notInstalledApp.menu,
    ],
    [
      "Unavailable",
      "Adding HUD tools requires Agent HUD at its approved installation location and its MCP components.",
      [],
    ],
  );

  // Added while the app-side switch is off: amber, and the row turns it on.
  const off = mcpRow(t, { ...mcp, added: true });
  assert.deepEqual(
    [off.tone, off.status, off.action?.id],
    ["attention", "Tools off", "turn-on"],
  );
  assert.deepEqual(
    off.menu.map((item) => item.id),
    ["copy", "remove"],
  );
  assert.equal(off.menu[1].label, "Remove from Codex");
  // In the demo the switch cannot change, and Turn on says why.
  const demo = toolsSwitch(t, {
    supported: true,
    enabled: false,
    listening: false,
    notice: null,
    demo: true,
  }).unavailable;
  assert.deepEqual(
    mcpRow(t, { ...mcp, added: true, toolsUnavailable: demo }).action
      ?.unavailable,
    "Not available in the demo.",
  );

  const failed = mcpRow(t, { ...mcp, added: true, toolsOn: true });
  assert.deepEqual(
    [failed.tone, failed.status, failed.action],
    ["failed", "Not running", null],
  );

  const ready = mcpRow(t, {
    ...mcp,
    added: true,
    toolsOn: true,
    listening: true,
  });
  assert.deepEqual(
    [ready.tone, ready.status, ready.action, ready.next],
    ["working", "Ready", null, null],
  );
  const justAdded = mcpRow(t, {
    ...mcp,
    client: "Claude Code",
    added: true,
    toolsOn: true,
    listening: true,
    justAdded: true,
  });
  assert.equal(
    justAdded.next?.words,
    "Start a new Claude Code session and ask it to change the HUD.",
  );
  // A copy confirms in place of any other next step.
  assert.equal(
    mcpRow(t, {
      ...mcp,
      added: true,
      toolsOn: true,
      listening: true,
      justAdded: true,
      copied: true,
    }).next?.words,
    "Command copied. Paste it into Terminal.",
  );

  assert.equal(mcpRow(t, { ...mcp, running: "add" }).busy, "Adding…");
  assert.deepEqual(
    mcpRow(t, {
      ...mcp,
      added: true,
      toolsOn: true,
      listening: true,
      running: "remove",
    }).busy,
    "Removing…",
  );
});

test("the switch explains itself only when it cannot change or the channel broke", () => {
  const healthy = {
    supported: true,
    enabled: true,
    listening: true,
    notice: null,
    demo: false,
  };
  assert.deepEqual(toolsSwitch(t, healthy), {
    unavailable: null,
    problem: null,
  });
  assert.deepEqual(
    toolsSwitch(t, { ...healthy, enabled: false, listening: false }),
    { unavailable: null, problem: null },
  );
  assert.deepEqual(
    toolsSwitch(t, { ...healthy, listening: false, notice: "socket in use" })
      .problem,
    "Not running: socket in use",
  );
  assert.equal(
    toolsSwitch(t, { ...healthy, demo: true }).unavailable,
    "Not available in the demo.",
  );
  const unsupported = toolsSwitch(t, {
    ...healthy,
    supported: false,
    enabled: false,
    listening: false,
  });
  assert.deepEqual(unsupported.unavailable, unsupported.problem);
});
