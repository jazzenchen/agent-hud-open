import { test } from "node:test";
import assert from "node:assert/strict";
import { chmodSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { EngineClient } from "../src/index.ts";

// A stand-in engine that answers in two writes split inside a character.
// It runs as a script with a shebang, which Windows does not run.
test(
  "an answer split inside a character arrives whole",
  { skip: process.platform === "win32" },
  async () => {
    const folder = mkdtempSync(join(tmpdir(), "engine-client-"));
    const engine = join(folder, "engine");
    writeFileSync(
      engine,
      `#!${process.execPath}
process.stdin.once("data", (line) => {
  const { id } = JSON.parse(String(line));
  const bytes = Buffer.from(JSON.stringify({ id, result: { title: "中文会话😀" } }) + "\\n");
  const cut = bytes.indexOf(Buffer.from("中")) + 1;
  process.stdout.write(bytes.subarray(0, cut));
  setTimeout(() => process.stdout.write(bytes.subarray(cut)), 50);
});
`,
    );
    chmodSync(engine, 0o755);
    const client = new EngineClient(engine);
    try {
      const result = await client.request<{ title: string }>("snapshot");
      assert.equal(result.title, "中文会话😀");
    } finally {
      client.close();
      rmSync(folder, { recursive: true, force: true });
    }
  },
);
