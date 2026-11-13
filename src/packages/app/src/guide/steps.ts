// The tour's steps in Settings: first the two pages worth a stop, then the
// switches that live there, each on its row. A product build adds its own
// after these (`ProductGuide.steps`).
import type { SettingsStep } from "./plan";

export const openSettingsSteps: SettingsStep[] = [
  {
    id: "display-page",
    page: "display",
    // The screen picker shows only with more than one screen.
    anchor: "screen-picker hud-mode",
    title: ["How the HUD looks", "HUD 的样式"],
    body: [
      "Each screen gets its own style, size and effects.",
      "每块屏幕都能单独设置样式、大小和显示效果。",
    ],
  },
  {
    id: "agents-page",
    page: "agents",
    anchor: "agents-list",
    title: ["Agents", "智能体"],
    body: [
      "Pick an agent here to choose whether it shows on the HUD, and which account and models it uses.",
      "在这里挑智能体，决定哪些显示在 HUD 上，各用哪个账户和模型。",
    ],
  },
  {
    id: "client-hooks",
    page: "agent-plugins",
    anchor: "client-hooks",
    title: ["Hooks and plugins", "回调和插件"],
    body: [
      "Turn it on so Agent HUD hears your agents' messages and permission requests.",
      "开启后，智能体的消息提醒和权限请求才会被 Agent HUD 收到。",
    ],
    done: (snapshot) => snapshot.settings.clientHooks === true,
  },
  {
    id: "hud-tools",
    page: "agent-plugins",
    anchor: "hud-tools",
    title: ["Let an agent restyle the HUD", "让智能体帮你美化 HUD"],
    body: [
      "Turn it on, then just talk to your agent and it changes the HUD's layout for you.",
      "开启后，直接跟你的智能体对话，它就能帮你改 HUD 的布局。",
    ],
    done: (snapshot) => snapshot.settings.hudTools === true,
  },
  {
    id: "agent-messages",
    page: "agent-plugins",
    anchor: "agent-messages",
    title: ["Let agents message each other", "让智能体互相发消息"],
    body: [
      "Turn it on and your agents talk to each other directly, with no copying and pasting between them.",
      "开启后，你的智能体之间就能直接对话，不用你来回复制粘贴转述。",
    ],
    done: (snapshot) => snapshot.settings.agentMessages === true,
  },
  {
    id: "client-tools",
    page: "agent-plugins",
    anchor: "clients",
    title: ["Turn them on in each client too", "别忘了在客户端里开启"],
    body: [
      "HUD tools and plugins are turned on in each client separately. See which ones are still missing here, and click Add.",
      "HUD 工具和插件要在每个客户端里分别开启。这里能看到哪个还没开启，点「添加」就行。",
    ],
  },
];
