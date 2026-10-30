import { test } from "node:test";
import assert from "node:assert/strict";
import { adoptEarlierKeys, storageKeys } from "../src/lib/storage-keys.ts";

function memory(entries: Record<string, string>): Storage {
  const values = new Map(Object.entries(entries));
  return {
    get length() {
      return values.size;
    },
    clear: () => values.clear(),
    getItem: (key) => values.get(key) ?? null,
    key: (index) => [...values.keys()][index] ?? null,
    removeItem: (key) => void values.delete(key),
    setItem: (key, value) => void values.set(key, value),
  };
}

test("an earlier build's values move to the new keys", () => {
  const storage = memory({
    "agenthud.next.hidden-agent-cards": '["codex"]',
    "agenthud.next.hud-config.v3": "{}",
    "agenthud.next.unrelated": "kept",
  });
  adoptEarlierKeys(storage);
  assert.equal(storage.getItem(storageKeys.hiddenAgentCards), '["codex"]');
  assert.equal(storage.getItem(storageKeys.hudConfig), "{}");
  assert.equal(storage.getItem("agenthud.next.hidden-agent-cards"), null);
  assert.equal(storage.getItem("agenthud.next.unrelated"), "kept");
});

test("a value already under the new key wins", () => {
  const storage = memory({
    "agenthud.selected-screen": "new",
    "agenthud.next.selected-screen": "old",
  });
  adoptEarlierKeys(storage);
  assert.equal(storage.getItem(storageKeys.selectedScreen), "new");
  assert.equal(storage.getItem("agenthud.next.selected-screen"), null);
});
