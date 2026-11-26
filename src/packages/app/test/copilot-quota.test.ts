// Turning on Copilot quota reading asks for confirmation each time and names
// the GitHub sign-in locations read on this operating system.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  copilotConsentBody,
  copilotConsentPlatform,
  copilotQuotaToggle,
} from "../src/settings/agents/copilot-quota.ts";

const identity = (text: string) => text;

test("switching Copilot quota from off to on requires confirmation", () => {
  assert.equal(copilotQuotaToggle(false, true), "confirm");
});

test("switching Copilot quota from on to off disables without confirmation", () => {
  assert.equal(copilotQuotaToggle(true, false), "disable");
});

test("repeated same-state switches are no-ops", () => {
  assert.equal(copilotQuotaToggle(false, false), "noop");
  assert.equal(copilotQuotaToggle(true, true), "noop");
});

test("platform resolution prefers platformCapabilities.os over connection.platform", () => {
  assert.equal(
    copilotConsentPlatform({
      platform: "linux",
      platformCapabilities: { os: "windows" },
    }),
    "windows",
  );
  assert.equal(
    copilotConsentPlatform({
      platform: "macos",
      platformCapabilities: {},
    }),
    "macos",
  );
  assert.equal(copilotConsentPlatform({ platform: "linux" }), "other");
  assert.equal(copilotConsentPlatform({}), "other");
});

test("macOS consent disclosure names keychain, gh hosts, Copilot files, and api.github.com", () => {
  const text = copilotConsentBody("macos", identity);
  assert.match(text, /GH_TOKEN/);
  assert.match(text, /GITHUB_TOKEN/);
  assert.match(text, /gh:github\.com/);
  assert.match(text, /macOS keychain/);
  assert.match(text, /~\/\.config\/gh\/hosts\.yml/);
  assert.match(text, /~\/\.config\/github-copilot\/apps\.json/);
  assert.match(text, /hosts\.json/);
  assert.match(text, /api\.github\.com/);
  assert.match(text, /macOS may ask for keychain access/);
});

test("Windows consent disclosure names Windows Credential Manager and AppData paths", () => {
  const text = copilotConsentBody("windows", identity);
  assert.match(text, /GH_TOKEN/);
  assert.match(text, /GITHUB_TOKEN/);
  assert.match(text, /%APPDATA%\\GitHub CLI\\hosts\.yml/);
  assert.match(text, /gh:github\.com/);
  assert.match(text, /Windows Credential Manager/);
  assert.match(text, /%LOCALAPPDATA%\\github-copilot\\apps\.json/);
  assert.match(text, /hosts\.json/);
  assert.match(text, /api\.github\.com/);
  assert.doesNotMatch(text, /macOS/i);
});

test("Linux and fallback consent disclosure names system credential store and XDG config paths", () => {
  const text = copilotConsentBody("other", identity);
  assert.match(text, /GH_TOKEN/);
  assert.match(text, /GITHUB_TOKEN/);
  assert.match(text, /gh:github\.com/);
  assert.match(text, /system credential store/);
  assert.match(text, /~\/\.config\/gh\/hosts\.yml/);
  assert.match(text, /~\/\.config\/github-copilot\/apps\.json/);
  assert.match(text, /hosts\.json/);
  assert.match(text, /api\.github\.com/);
  assert.doesNotMatch(text, /macOS|Windows/i);
});
