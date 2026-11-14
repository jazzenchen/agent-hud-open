// What each release adds, as the update card tells it: two or three items a
// release, written when it is cut; a release with nothing to tell has no
// entry, and no card opens. A product build adds its own (`AppProps.news`).
// A target names where "Show me" goes: a Settings page, and the element on
// it marked `data-guide`.
import type { NewsEntry } from "./plan";

export const openNews: NewsEntry[] = [
  {
    version: "0.6.3",
    items: [
      {
        title: ["More accounts found on their own", "更多账号自动认出"],
        body: [
          "Qoder, QoderWork, TRAE, WorkBuddy, CodeBuddy, ZCode and MiniMax Code accounts come from each app's own sign-in, each region on its own.",
          "Qoder、QoderWork、TRAE、WorkBuddy、CodeBuddy、ZCode 和 MiniMax Code 的账号直接从各自应用的登录里读，不同区域分开显示。",
        ],
      },
      {
        title: [
          "Agents can start DeepSeek Harness sessions",
          "Agent 可以新开 DeepSeek Harness 会话",
        ],
        body: [
          "Through Agent HUD's plugin in a running Harness.",
          "经由运行中 Harness 里的 Agent HUD 插件。",
        ],
      },
    ],
  },
  {
    version: "0.6.2",
    items: [
      {
        title: [
          "A guide beside the HUD on first launch, and this note after each update",
          "第一次打开时 HUD 旁会带你走一遍，每次更新后在这里说新增了什么",
        ],
        body: [
          "Both open again from the ⋯ menu in Settings.",
          "两样都能从设置里的 ⋯ 菜单再打开。",
        ],
        target: { anchor: "more-menu" },
      },
    ],
  },
];
