// OpenCode 1 calls the default export as its plugin; before 1.2.11, an export it cannot call stops OpenCode itself.
// OpenCode 2 refuses a default export that is not an object with an id and `setup`, and gives its plugins
// `@opencode/plugin`, which OpenCode 1 does not have.
let plugin = server;
try {
  await import("@opencode/plugin");
  plugin = { id: "agent-hud", setup, server };
} catch { /* OpenCode 1 */ }
export { plugin as default };
