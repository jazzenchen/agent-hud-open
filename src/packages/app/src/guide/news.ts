// What each release adds, as the update card tells it: two or three items a
// release, written when it is cut; a release with nothing to tell has no
// entry, and no card opens. A product build adds its own (`AppProps.news`).
// A target names where "Show me" goes: a Settings page, and the element on
// it marked `data-guide`.
import type { NewsEntry } from "./plan";

export const openNews: NewsEntry[] = [
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
