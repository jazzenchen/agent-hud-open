// The Settings window's pages: the built-in ones and their sidebar groups,
// and where a product's own pages go among them.
import type { ReactNode } from "react";
import {
  Coins,
  Download,
  Monitor,
  Network,
  Plug,
  Puzzle,
  Settings2,
  Waypoints,
} from "lucide-react";
import type { Snapshot } from "@agent-hud/protocol";
import { isChinese, type Language, type Translate } from "@agent-hud/ui/format";

/** Sidebar groups, top to bottom; a gap separates them. */
export const pageGroups = ["app", "agents", "devices", "about"] as const;
export type PageGroup = (typeof pageGroups)[number];
const builtInPages = (
  t: Translate,
): {
  id: string;
  label: string;
  icon: typeof Settings2;
  color: string;
  description: string;
  group: PageGroup;
}[] => [
  {
    id: "general",
    label: t("General"),
    icon: Settings2,
    color: "#8e8e93",
    description: t("Appearance, language and startup."),
    group: "app",
  },
  {
    id: "display",
    label: t("Display"),
    icon: Monitor,
    color: "#7c65e8",
    description: t("The HUD and its panel on each screen."),
    group: "app",
  },
  {
    id: "agents",
    label: t("Agents"),
    icon: Puzzle,
    color: "#30b0c7",
    description: t("Which agents Agent HUD follows, and their accounts."),
    group: "agents",
  },
  {
    // What Agent HUD installs into each client.
    id: "agent-plugins",
    label: t("Agent plugins"),
    icon: Plug,
    color: "#30b0c7",
    description: t("What Agent HUD adds to the agents on this computer."),
    group: "agents",
  },
  {
    // Relays, one per address, whichever agents use them.
    id: "relays",
    label: t("Relays"),
    icon: Waypoints,
    color: "#ff9500",
    description: t("The relays your agents go through."),
    group: "agents",
  },
  {
    id: "prices",
    label: t("Prices"),
    icon: Coins,
    color: "#34c759",
    description: t("What each model costs. Set your own when it is wrong."),
    group: "agents",
  },
  {
    // What paired phones read: quota and token usage, never sessions.
    id: "local-network",
    label: t("Local Network"),
    icon: Network,
    color: "#5e5ce6",
    description: t("Share quota and token usage with paired phones."),
    group: "devices",
  },
  {
    id: "updates",
    label: t("Updates"),
    icon: Download,
    color: "#007aff",
    description: t("Agent HUD updates and beta releases."),
    group: "about",
  },
];

export interface AppExtensionPage {
  id: string;
  title: string;
  icon?: ReactNode;
  color?: string;
  description?: string;
  /** The title and description in Chinese (transitional, until products
   * use catalogs). Without it, `title` and `description` are catalog keys. */
  zh?: [string, string];
  /** Built-in page id this page takes the place of, in its slot. */
  replaces?: string;
  /** Sidebar group of a page that replaces none; the last group if unset. */
  group?: PageGroup;
  /** Page id this page goes right before, within its group. */
  before?: string;
  render(context: {
    snapshot: Snapshot | null;
    request: (method: string, params?: unknown) => Promise<unknown>;
  }): ReactNode;
}
type SidebarPage = {
  id: string;
  label: string;
  icon: typeof Settings2;
  color: string;
  description: string;
  group: PageGroup;
  customIcon?: ReactNode;
};
/** The sidebar's pages in order, in the interface language: a product page
 * that replaces a built-in one takes its slot; the others join their group,
 * before the page they name or at the group's end. */
export function sidebarPages(
  extensionPages: AppExtensionPage[],
  t: Translate,
  language: Language,
): SidebarPage[] {
  const chinese = (page: AppExtensionPage) =>
    isChinese(language) ? page.zh : undefined;
  const list: SidebarPage[] = builtInPages(t).map((page) => {
    const replacement = extensionPages.find(
      (item) => item.replaces === page.id,
    );
    return replacement
      ? {
          id: replacement.id,
          label: chinese(replacement)?.[0] ?? t(replacement.title),
          icon: page.icon,
          color: replacement.color ?? page.color,
          description:
            chinese(replacement)?.[1] ??
            (replacement.description
              ? t(replacement.description)
              : page.description),
          group: page.group,
          customIcon: replacement.icon,
        }
      : { ...page };
  });
  for (const page of extensionPages) {
    if (page.replaces) continue;
    const entry: SidebarPage = {
      id: page.id,
      label: chinese(page)?.[0] ?? t(page.title),
      icon: Puzzle,
      color: page.color ?? "#7474d4",
      description:
        chinese(page)?.[1] ?? (page.description ? t(page.description) : ""),
      group: page.group ?? pageGroups[pageGroups.length - 1],
      customIcon: page.icon,
    };
    const before = page.before
      ? list.findIndex((item) => item.id === page.before)
      : -1;
    if (before >= 0) {
      list.splice(before, 0, { ...entry, group: list[before].group });
      continue;
    }
    let last = -1;
    list.forEach((item, index) => {
      if (pageGroups.indexOf(item.group) <= pageGroups.indexOf(entry.group))
        last = index;
    });
    list.splice(last + 1, 0, entry);
  }
  return list;
}
