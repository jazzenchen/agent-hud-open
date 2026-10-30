import { test } from "node:test";
import assert from "node:assert/strict";
import {
  mcpRow,
  pluginRow,
  toolsSwitch,
  type McpInput,
} from "../src/lib/agent-plugins.ts";

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
  const reading = pluginRow({ ...plugin, installed: null });
  assert.deepEqual([reading.status[0], reading.action], ["Checking…", null]);

  const missing = pluginRow(plugin);
  assert.deepEqual(
    [
      missing.tone,
      missing.status,
      missing.action?.id,
      missing.action?.unavailable,
    ],
    ["off", ["Not installed", "未安装"], "install", undefined],
  );
  assert.deepEqual(missing.menu, []);

  const noClient = pluginRow({ ...plugin, clientFound: false });
  assert.deepEqual(noClient.action?.unavailable, [
    "Install Claude Code first.",
    "请先安装 Claude Code。",
  ]);

  const installing = pluginRow({ ...plugin, running: "install" });
  assert.deepEqual(installing.busy, ["Installing…", "正在安装…"]);

  // In place but no session has loaded it: blue, no button, and the step that loads it.
  const idle = pluginRow({ ...plugin, installed: true });
  assert.deepEqual(
    [idle.tone, idle.status[0], idle.action],
    ["installed", "Installed", null],
  );
  assert.equal(idle.next?.command, "/reload-plugins");
  assert.match(
    idle.next!.words[1],
    /已经打开的会话，在其中运行 \/reload-plugins/,
  );
  assert.deepEqual(
    idle.menu.map((item) => [item.id, item.destructive]),
    [["remove", true]],
  );

  const loaded = pluginRow({ ...plugin, installed: true, sessions: 2 });
  assert.deepEqual(
    [loaded.tone, loaded.status],
    ["working", ["Loaded in 2 sessions", "已在 2 个会话中加载"]],
  );
  assert.equal(loaded.next, null);
  assert.equal(
    pluginRow({ ...plugin, installed: true, sessions: 1 }).status[0],
    "Loaded in 1 session",
  );
  // Right after installing, the reload step stays even once a session loaded it.
  assert.ok(
    pluginRow({ ...plugin, installed: true, sessions: 1, justInstalled: true })
      .next,
  );

  assert.deepEqual(
    pluginRow({ ...plugin, installed: true, sessions: 2, running: "remove" })
      .busy,
    ["Removing…", "正在移除…"],
  );
});

test("a plugin the client loads by itself only asks to open the client, never to reload", () => {
  const client = "DeepSeek Harness";
  assert.equal(
    pluginRow({ ...plugin, clientFound: false }, client, true).action
      ?.unavailable?.[0],
    "Install DeepSeek Harness first.",
  );
  const idle = pluginRow({ ...plugin, installed: true }, client, true);
  assert.equal(idle.status[0], "Installed");
  assert.equal(idle.next?.command, undefined);
  assert.match(
    idle.next?.words[0] ?? "",
    /^DeepSeek Harness loads it by itself/,
  );
  assert.match(idle.next?.words[1] ?? "", /打开 DeepSeek Harness/);
  const loaded = pluginRow(
    { ...plugin, installed: true, sessions: 2, justInstalled: true },
    client,
    true,
  );
  assert.deepEqual([loaded.tone, loaded.next], ["working", null]);
  assert.equal(loaded.menu[0]?.id, "remove");
});

test("the MCP row asks for the one step left, and never removes from its main button", () => {
  assert.equal(mcpRow({ ...mcp, setupReady: null }).status[0], "Checking…");

  const notAdded = mcpRow(mcp);
  assert.deepEqual(
    [
      notAdded.tone,
      notAdded.status[0],
      notAdded.action?.id,
      notAdded.action?.unavailable,
    ],
    ["off", "Not added", "add", undefined],
  );
  assert.deepEqual(
    notAdded.menu.map((item) => item.id),
    ["copy"],
  );

  const noClient = mcpRow({ ...mcp, clientFound: false });
  assert.deepEqual(noClient.action?.unavailable, [
    "Install Codex first.",
    "请先安装 Codex。",
  ]);

  const notInstalledApp = mcpRow({
    ...mcp,
    setupReady: false,
    hasCommand: false,
  });
  assert.deepEqual(
    [
      notInstalledApp.status[0],
      notInstalledApp.action?.unavailable?.[0],
      notInstalledApp.menu,
    ],
    [
      "Unavailable",
      "Adding HUD tools requires Agent HUD at its approved installation location and its MCP components.",
      [],
    ],
  );

  // Added while the app-side switch is off: amber, and the row turns it on.
  const off = mcpRow({ ...mcp, added: true });
  assert.deepEqual(
    [off.tone, off.status, off.action?.id],
    ["attention", ["Tools off", "工具已关闭"], "turn-on"],
  );
  assert.deepEqual(
    off.menu.map((item) => item.id),
    ["copy", "remove"],
  );
  assert.deepEqual(off.menu[1].label, ["Remove from Codex", "从 Codex 移除"]);
  // In the demo the switch cannot change, and Turn on says why.
  const demo = toolsSwitch({
    supported: true,
    enabled: false,
    listening: false,
    notice: null,
    demo: true,
  }).unavailable;
  assert.deepEqual(
    mcpRow({ ...mcp, added: true, toolsUnavailable: demo }).action?.unavailable,
    ["Not available in the demo.", "演示模式下不可用。"],
  );

  const failed = mcpRow({ ...mcp, added: true, toolsOn: true });
  assert.deepEqual(
    [failed.tone, failed.status[0], failed.action],
    ["failed", "Not running", null],
  );

  const ready = mcpRow({ ...mcp, added: true, toolsOn: true, listening: true });
  assert.deepEqual(
    [ready.tone, ready.status, ready.action, ready.next],
    ["working", ["Ready", "可以使用"], null, null],
  );
  const justAdded = mcpRow({
    ...mcp,
    client: "Claude Code",
    added: true,
    toolsOn: true,
    listening: true,
    justAdded: true,
  });
  assert.deepEqual(justAdded.next?.words, [
    "Start a new Claude Code session and ask it to change the HUD.",
    "在 Claude Code 里新开一个会话，说“把 HUD 改成……”。",
  ]);
  // A copy confirms in place of any other next step.
  assert.equal(
    mcpRow({
      ...mcp,
      added: true,
      toolsOn: true,
      listening: true,
      justAdded: true,
      copied: true,
    }).next?.words[0],
    "Command copied. Paste it into Terminal.",
  );

  assert.deepEqual(mcpRow({ ...mcp, running: "add" }).busy, [
    "Adding…",
    "正在添加…",
  ]);
  assert.deepEqual(
    mcpRow({
      ...mcp,
      added: true,
      toolsOn: true,
      listening: true,
      running: "remove",
    }).busy,
    ["Removing…", "正在移除…"],
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
  assert.deepEqual(toolsSwitch(healthy), { unavailable: null, problem: null });
  assert.deepEqual(
    toolsSwitch({ ...healthy, enabled: false, listening: false }),
    { unavailable: null, problem: null },
  );
  assert.deepEqual(
    toolsSwitch({ ...healthy, listening: false, notice: "socket in use" })
      .problem,
    ["Not running: socket in use", "未运行：socket in use"],
  );
  assert.deepEqual(toolsSwitch({ ...healthy, demo: true }).unavailable, [
    "Not available in the demo.",
    "演示模式下不可用。",
  ]);
  const unsupported = toolsSwitch({
    ...healthy,
    supported: false,
    enabled: false,
    listening: false,
  });
  assert.deepEqual(unsupported.unavailable, unsupported.problem);
});
