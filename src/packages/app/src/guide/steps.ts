// The tour's steps in Settings: first the two pages worth a stop, then the
// switches that live there, each on its row. A product build adds its own
// after these (`ProductGuide.steps`).
import type { SettingsStep } from "./plan";

/** Marks English text as a catalog key; the guide translates it where it
 * shows it. */
const t = (key: string) => key;

export const openSettingsSteps: SettingsStep[] = [
  {
    id: "display-page",
    page: "display",
    // The screen picker shows only with more than one screen.
    anchor: "screen-picker hud-mode",
    title: t("How the HUD looks"),
    body: t("Each screen gets its own style, size and effects."),
  },
  {
    id: "agents-page",
    page: "agents",
    anchor: "agents-list",
    title: t("Agents"),
    body: t(
      "Pick an agent here to choose whether it shows on the HUD, and which account and models it uses.",
    ),
  },
  {
    id: "client-hooks",
    page: "agent-plugins",
    anchor: "client-hooks",
    title: t("Hooks and plugins"),
    body: t(
      "Turn it on so Agent HUD hears your agents' messages and permission requests.",
    ),
    done: (snapshot) => snapshot.settings.clientHooks === true,
  },
  {
    id: "hud-tools",
    page: "agent-plugins",
    anchor: "hud-tools",
    title: t("Let an agent restyle the HUD"),
    body: t(
      "Turn it on, then just talk to your agent and it changes the HUD's layout for you.",
    ),
    done: (snapshot) => snapshot.settings.hudTools === true,
  },
  {
    id: "agent-messages",
    page: "agent-plugins",
    anchor: "agent-messages",
    title: t("Let agents message each other"),
    body: t(
      "Turn it on and your agents talk to each other directly, with no copying and pasting between them.",
    ),
    done: (snapshot) => snapshot.settings.agentMessages === true,
  },
  {
    id: "client-tools",
    page: "agent-plugins",
    anchor: "clients",
    title: t("Turn them on in each client too"),
    body: t(
      "HUD tools and plugins are turned on in each client separately. See which ones are still missing here, and click Add.",
    ),
  },
];
