/* Copilot quota reading uses existing GitHub sign-ins on this computer, so
 * turning it on confirms what is read on the current operating system. */
import type { Translate } from "@agent-hud/ui/format";

export type ConsentPlatform = "macos" | "windows" | "other";
export type CopilotToggleAction = "confirm" | "disable" | "noop";

/** Turning the switch on asks for consent; turning it off saves at once. */
export function copilotQuotaToggle(
  current: boolean,
  next: boolean,
): CopilotToggleAction {
  if (current === next) return "noop";
  return next ? "confirm" : "disable";
}

/** The host platform reported by the connected runtime, for accurate
 * credential-source disclosure. */
export function copilotConsentPlatform(connection?: {
  platform?: string;
  platformCapabilities?: { os?: string };
}): ConsentPlatform {
  const os = connection?.platformCapabilities?.os ?? connection?.platform;
  if (os === "macos" || os === "windows") return os;
  return "other";
}

/** What Copilot's quota reader inspects on `platform` once allowed. */
export function copilotConsentBody(
  platform: ConsentPlatform,
  t: Translate,
): string {
  if (platform === "macos") {
    return t(
      "This reads your GitHub sign-in (GH_TOKEN or GITHUB_TOKEN, gh:github.com in the macOS keychain, ~/.config/gh/hosts.yml, or ~/.config/github-copilot/apps.json and hosts.json) to query Copilot quota from api.github.com. macOS may ask for keychain access.",
    );
  }
  if (platform === "windows") {
    return t(
      "This reads your GitHub sign-in (GH_TOKEN or GITHUB_TOKEN, %APPDATA%\\GitHub CLI\\hosts.yml, gh:github.com in Windows Credential Manager, or %LOCALAPPDATA%\\github-copilot\\apps.json and hosts.json) to query Copilot quota from api.github.com.",
    );
  }
  return t(
    "This reads your GitHub sign-in (GH_TOKEN or GITHUB_TOKEN, gh:github.com in the system credential store, ~/.config/gh/hosts.yml, or ~/.config/github-copilot/apps.json and hosts.json) to query Copilot quota from api.github.com.",
  );
}
