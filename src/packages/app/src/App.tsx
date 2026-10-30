import { FormatProvider } from "@agent-hud/ui/format-context";
import {
  Component,
  lazy,
  Suspense,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { currentMonitor, getCurrentWindow } from "@tauri-apps/api/window";
import { emit } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import {
  CircleHelp,
  Coins,
  Download,
  Monitor,
  MoreHorizontal,
  Network,
  Plug,
  Puzzle,
  Settings2,
  Waypoints,
} from "lucide-react";
import { toast, Toaster } from "sonner";
import { Button } from "@agent-hud/ui/components/button";
import { Tabs, TabsList, TabsTrigger } from "@agent-hud/ui/components/tabs";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@agent-hud/ui/components/dropdown-menu";
import { TooltipProvider } from "@agent-hud/ui/components/tooltip";
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@agent-hud/ui/components/alert";
import {
  PanelServicesProvider,
  type PanelServices,
} from "@agent-hud/hud/PanelContext";
import { useHUD } from "./lib/use-hud";
import { useWindowActivity } from "./lib/use-window-activity";
import { useBotDirections } from "./lib/use-bot-directions";
import {
  getScreenHUDConfig,
  useHUDConfig,
  type HUDScreenContext,
} from "./lib/use-hud-config";
import { useHUDGeometry } from "./lib/use-hud-geometry";
import { storageKeys } from "./lib/storage-keys";
import type { HUDConfig } from "@agent-hud/hud/model";
import type { NativeMethod, Session, Snapshot } from "./lib/types";
import appIcon from "./assets/app-icon.png";
import { snapshotLanguage } from "@agent-hud/ui/format";
import type { AgentPluginItem } from "./settings/AgentPluginsPane";
import { SettingsNavigation } from "./settings/SettingsNavigation";
import { LoadingState } from "./LoadingState";

export type {
  AgentPluginContext,
  AgentPluginItem,
} from "./settings/AgentPluginsPane";

// Windows load their own views. Statistics does not need the shader/bot renderer,
// and a resting HUD does not need the historical charts and settings panes.
const NativeHUDSurface = lazy(() => import("./HUDSurface"));
const Statistics = lazy(() => import("./Statistics"));
const SessionStatistics = lazy(() => import("./SessionStatistics"));
const SettingsPane = lazy(() => import("./SettingsPanes"));

class ViewErrorBoundary extends Component<
  { children: ReactNode; onReset?: () => void; chinese?: boolean },
  { error: Error | null }
> {
  state: { error: Error | null } = { error: null };
  static getDerivedStateFromError(error: Error) {
    return { error };
  }
  render() {
    if (!this.state.error) return this.props.children;
    return (
      <div
        className="connection-placeholder"
        style={{ flexDirection: "column", gap: 10 }}
      >
        <span>
          {this.props.chinese
            ? "无法显示这个页面："
            : "Failed to render view: "}
          {this.state.error.message}
        </span>
        <Button
          variant="outline"
          size="sm"
          onClick={() => {
            this.setState({ error: null });
            this.props.onReset?.();
          }}
        >
          {this.props.chinese ? "返回" : "Back"}
        </Button>
      </div>
    );
  }
}

type Page = string;
/** Sidebar groups, top to bottom; a gap separates them. */
const pageGroups = ["app", "agents", "devices", "about"] as const;
type PageGroup = (typeof pageGroups)[number];
const pages: {
  id: string;
  label: string;
  icon: typeof Settings2;
  color: string;
  description: string;
  group: PageGroup;
}[] = [
  {
    id: "general",
    label: "General",
    icon: Settings2,
    color: "#8e8e93",
    description: "Appearance, language and startup.",
    group: "app",
  },
  {
    id: "display",
    label: "Display",
    icon: Monitor,
    color: "#7c65e8",
    description: "The HUD and its panel on each screen.",
    group: "app",
  },
  {
    id: "agents",
    label: "Agents",
    icon: Puzzle,
    color: "#30b0c7",
    description: "Which agents Agent HUD follows, and their accounts.",
    group: "agents",
  },
  {
    // What Agent HUD installs into each client.
    id: "agent-plugins",
    label: "Agent plugins",
    icon: Plug,
    color: "#30b0c7",
    description: "What Agent HUD adds to the agents on this computer.",
    group: "agents",
  },
  {
    // Relays, one per address, whichever agents use them.
    id: "relays",
    label: "Relays",
    icon: Waypoints,
    color: "#ff9500",
    description: "The relays your agents go through.",
    group: "agents",
  },
  {
    id: "prices",
    label: "Prices",
    icon: Coins,
    color: "#34c759",
    description: "What each model costs. Set your own when it is wrong.",
    group: "agents",
  },
  {
    // What paired phones read: quota and token usage, never sessions.
    id: "local-network",
    label: "Local Network",
    icon: Network,
    color: "#5e5ce6",
    description: "Share quota and token usage with paired phones.",
    group: "devices",
  },
  {
    id: "updates",
    label: "Updates",
    icon: Download,
    color: "#007aff",
    description: "Agent HUD updates and beta releases.",
    group: "about",
  },
];
const chinesePages: Record<string, [string, string]> = {
  general: ["通用", "外观、语言与启动。"],
  display: ["显示", "每块屏幕上的 HUD 和它的面板。"],
  agents: ["智能体", "Agent HUD 关注哪些智能体，以及它们的账户。"],
  "agent-plugins": ["智能体插件", "Agent HUD 为这台电脑上的智能体添加的功能。"],
  relays: ["中转站", "智能体经过的中转站。"],
  prices: ["价格", "每个模型的价格，不对时可以自己改。"],
  "local-network": ["局域网", "与已配对的手机共享额度和 token 用量。"],
  updates: ["更新", "Agent HUD 的更新与 Beta 版本。"],
};
const selectedScreenKey = storageKeys.selectedScreen;

export interface AppExtensionPage {
  id: string;
  title: string;
  icon?: ReactNode;
  color?: string;
  description?: string;
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
export interface AppProps {
  extensionPages?: AppExtensionPage[];
  /** Rows a product build adds to a client's card on Agent plugins. */ agentPlugins?: AgentPluginItem[];
  productName?: string;
  version?: string;
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
/** The sidebar's pages in order: a product page that replaces a built-in
 * one takes its slot; the others join their group, before the page they
 * name or at the group's end. */
function sidebarPages(extensionPages: AppExtensionPage[]): SidebarPage[] {
  const list: SidebarPage[] = pages.map((page) => {
    const replacement = extensionPages.find(
      (item) => item.replaces === page.id,
    );
    return replacement
      ? {
          id: replacement.id,
          label: replacement.title,
          icon: page.icon,
          color: replacement.color ?? page.color,
          description: replacement.description ?? page.description,
          group: page.group,
          customIcon: replacement.icon,
        }
      : { ...page };
  });
  for (const page of extensionPages) {
    if (page.replaces) continue;
    const entry: SidebarPage = {
      id: page.id,
      label: page.title,
      icon: Puzzle,
      color: page.color ?? "#7474d4",
      description: page.description ?? "",
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
const PictureView = lazy(() => import("./HUDPicture"));
export default function App(props: AppProps = {}) {
  if (new URLSearchParams(location.search).get("surface") === "picture")
    return <PictureView />;
  return <AppContent {...props} />;
}
function AppContent({
  extensionPages = [],
  agentPlugins = [],
  productName = "Agent HUD",
  version = "0.1.0",
}: AppProps) {
  const allPages = useMemo(
    () => sidebarPages(extensionPages),
    [extensionPages],
  );
  for (const page of extensionPages)
    if (page.zh) chinesePages[page.id] = page.zh;
  const parameters = new URLSearchParams(location.search);
  const surface = parameters.get("surface") === "hud";
  const statistics = parameters.get("window") === "statistics";
  const settings = !surface && !statistics;
  // The app plans the HUD windows, draws the menu bar and directs the Bots
  // itself. A page listens only while its window is on screen, and reads what
  // it missed when it is back.
  const windowActive = useWindowActivity();
  const {
    snapshot: collectedSnapshot,
    loading,
    error,
    demo,
    refresh,
    act,
  } = useHUD({
    active: windowActive,
  });
  const extensionRequest = (method: string, params: unknown = {}) =>
    act("extension", { method, params });
  const surfaceScreenId = surface
    ? (parameters.get("screen") ?? "")
    : undefined;
  const { geometries, monitors } = useHUDGeometry(
    surfaceScreenId,
    windowActive,
  );
  const [preferredScreenId, setPreferredScreenId] = useState(
    () => localStorage.getItem(selectedScreenKey) || "",
  );
  const selectedId = surface
    ? surfaceScreenId!
    : (monitors.find((monitor) => monitor.screenId === preferredScreenId)
        ?.screenId ??
      monitors.find((monitor) => monitor.primary)?.screenId ??
      monitors[0]?.screenId ??
      "");
  const screenContexts = useMemo<HUDScreenContext[]>(
    () =>
      monitors.map((monitor) => ({
        id: monitor.screenId,
        monitorIndex: monitor.index,
        hasNotch: monitor.notch !== null,
      })),
    [monitors],
  );
  const {
    config: storedConfig,
    store,
    ready,
    update: updateConfig,
    presentation,
    setLayout,
    setSource,
    saveDraft,
    discardDraft,
    releaseHold,
  } = useHUDConfig(selectedId, screenContexts, windowActive);
  const snapshot = useMemo(() => {
    if (!collectedSnapshot) return null;
    const draft = store.appDraft as Record<string, any> | null | undefined;
    const settings = {
      ...collectedSnapshot.settings,
      ...(draft?.appearance !== undefined
        ? { appearance: draft.appearance }
        : {}),
      ...(draft?.matchScreen !== undefined
        ? { appearanceFollowsScreen: draft.matchScreen }
        : {}),
      ...(draft?.showMenuBarIcon !== undefined
        ? { showMenuBarIcon: draft.showMenuBarIcon }
        : {}),
    };
    return { ...collectedSnapshot, settings };
  }, [collectedSnapshot, store.appDraft, store.sources]);
  const seenRequestDeliveries = useRef(new Set<string>());
  useEffect(() => {
    if (!settings || !windowActive) return;
    for (const delivery of snapshot?.requestDeliveries ?? []) {
      const key = `${delivery.id}:${delivery.state}`;
      if (seenRequestDeliveries.current.has(key)) continue;
      seenRequestDeliveries.current.add(key);
      if (
        delivery.state === "confirmed" ||
        Date.now() - Date.parse(delivery.at) > 300_000
      )
        continue;
      const zh = snapshot?.language === "zh";
      const title =
        delivery.state === "uncertain"
          ? zh
            ? "未能确认客户端已收到回应"
            : "Client response unconfirmed"
          : zh
            ? "客户端回应失败"
            : "Client response failed";
      const description =
        delivery.reason ??
        (zh
          ? "请先在原客户端中查看，再做下一步操作。"
          : "Check the original client before taking another action.");
      if (delivery.state === "uncertain") toast.warning(title, { description });
      else toast.error(title, { description });
    }
  }, [settings, windowActive, snapshot?.requestDeliveries]);
  const botDirections = useBotDirections(surface && windowActive);
  const measured = geometries[selectedId] ?? null;
  const selectedMonitor = monitors.find(
    (monitor) => monitor.screenId === selectedId,
  );
  const config: HUDConfig = {
    ...storedConfig,
    hasNotch: measured?.hasNotch ?? Boolean(selectedMonitor?.notch),
    notchWidth: measured?.notch?.width ?? selectedMonitor?.notch?.width ?? 200,
    notchHeight:
      measured?.notch?.height ??
      selectedMonitor?.notch?.height ??
      measured?.menuBarHeight ??
      24,
  };
  const glowHidden = presentation?.glowHidden ?? false;
  const [page, setPage] = useState<Page>("display");
  // The running app's own version: a release is stamped with its version when
  // it is built (0.6.0-beta.1), which a product's front end cannot know.
  // `version` stands in outside the app.
  const [runningVersion, setRunningVersion] = useState(version);
  useEffect(() => {
    if (!isTauri()) return;
    void getVersion()
      .then(setRunningVersion)
      .catch(() => {});
  }, []);
  // A page the host asked for (the update notice opens Updates): taken when
  // Settings starts and whenever the host asks again. A product page that
  // takes a built-in page's place answers for it.
  useEffect(() => {
    if (!isTauri() || !settings) return;
    const open = (id: string | null) => {
      if (!id) return;
      const target =
        extensionPages.find((page) => page.replaces === id)?.id ?? id;
      if (allPages.some((page) => page.id === target)) setPage(target);
    };
    const take = () =>
      void invoke<string | null>("take_settings_page")
        .then(open)
        .catch(() => {});
    take();
    const subscription = getCurrentWebviewWindow().listen<string>(
      "settings:page",
      take,
    );
    return () => void subscription.then((unlisten) => unlisten());
  }, []);
  const [statisticsTab, setStatisticsTab] = useState("tokens");
  const [selectedSession, setSelectedSession] = useState<Session | null>(null);
  const hudVisible = snapshot?.settings.hudVisible !== false;
  const [systemLight, setSystemLight] = useState(
    () => matchMedia("(prefers-color-scheme: light)").matches,
  );
  function selectScreen(id: string) {
    setPreferredScreenId(id);
    localStorage.setItem(selectedScreenKey, id);
  }
  function toggleHUDVisibility() {
    void action("set_setting", { key: "hudVisible", value: !hudVisible });
  }
  useEffect(() => {
    if (settings && selectedId && !preferredScreenId) selectScreen(selectedId);
  }, [settings, selectedId, preferredScreenId]);
  useEffect(() => {
    const media = matchMedia("(prefers-color-scheme: light)");
    const change = () => setSystemLight(media.matches);
    media.addEventListener("change", change);
    return () => media.removeEventListener("change", change);
  }, []);
  const [windowScreen, setWindowScreen] = useState<string | null>(null);
  useEffect(() => {
    if (!isTauri() || surface || !snapshot?.settings.appearanceFollowsScreen)
      return;
    let disposed = false;
    const window = getCurrentWindow();
    const read = () =>
      void currentMonitor().then((monitor) => {
        if (disposed || !monitor) return;
        const match = monitors.find(
          (m) =>
            m.bounds.x === monitor.position.x / monitor.scaleFactor &&
            m.bounds.y === monitor.position.y / monitor.scaleFactor,
        );
        setWindowScreen(match?.screenId ?? null);
      });
    read();
    const moved = window.onMoved(read);
    return () => {
      disposed = true;
      void moved.then((unlisten) => unlisten());
    };
  }, [surface, monitors, snapshot?.settings.appearanceFollowsScreen]);
  const appearance =
    snapshot?.settings.appearanceFollowsScreen && windowScreen
      ? getScreenHUDConfig(store, {
          id: windowScreen,
          monitorIndex: 0,
          hasNotch: false,
        }).appearance
      : snapshot?.settings.appearance || "system";
  const chinese = snapshotLanguage(snapshot) === "zh";
  const theme =
    appearance === "system" ? (systemLight ? "light" : "dark") : appearance;
  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    document.documentElement.classList.toggle("dark", theme === "dark");
    document.documentElement.classList.toggle("native-hud-document", surface);
  }, [theme, surface]);
  useEffect(() => {
    setSelectedSession(null);
  }, [demo]);
  useEffect(() => {
    if (!statistics || !isTauri()) return;
    const subscription = getCurrentWebviewWindow().listen<string>(
      "statistics:tab",
      (event) => setStatisticsTab(event.payload),
    );
    return () => {
      void subscription.then((unlisten) => unlisten());
    };
  }, [statistics]);
  async function action(
    method: NativeMethod,
    params?: Record<string, unknown>,
  ) {
    try {
      return await act(method, params);
    } catch (failure) {
      toast.error(String(failure));
      throw failure;
    }
  }
  async function showStatistics(tab = "tokens") {
    if (isTauri()) {
      await invoke("open_statistics", { tab });
    } else {
      window.open(`?window=statistics`, "_blank");
    }
  }
  const sourceMenu = (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="ghost"
          size="icon"
          aria-label={chinese ? "更多选项" : "More options"}
        >
          <MoreHorizontal size={16} />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuItem
          onClick={() => {
            void showStatistics();
          }}
        >
          {chinese ? "用量统计…" : "Statistics…"}
        </DropdownMenuItem>
        {statistics && (
          <DropdownMenuItem
            onClick={() => {
              if (isTauri()) void invoke("hud_show_main");
            }}
          >
            {chinese ? "设置…" : "Settings…"}
          </DropdownMenuItem>
        )}
        <DropdownMenuSeparator />
        <DropdownMenuItem onClick={toggleHUDVisibility}>
          {hudVisible
            ? chinese
              ? "隐藏 HUD"
              : "Hide HUD"
            : chinese
              ? "显示 HUD"
              : "Show HUD"}
        </DropdownMenuItem>
        {isTauri() && (
          <>
            <DropdownMenuSeparator />
            <DropdownMenuItem
              onClick={() => {
                void invoke("quit_app");
              }}
            >
              {chinese ? "退出" : "Quit"}
            </DropdownMenuItem>
          </>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
  const sessionsRef = useRef<Session[]>([]);
  sessionsRef.current = snapshot?.sessions ?? [];
  useEffect(() => {
    if (!statistics || !isTauri()) return;
    const applySession = (id: string) => {
      const session = sessionsRef.current.find((s) => s.id === id);
      setSelectedSession(session ?? ({ id } as Session));
      setStatisticsTab("sessions");
    };
    const pendingSession = getCurrentWebviewWindow().listen<string>(
      "statistics:session",
      (event) => {
        applySession(event.payload);
      },
    );
    const pendingProvider = getCurrentWebviewWindow().listen<{
      provider: string;
      at: number;
    }>("statistics:provider", () => {
      setSelectedSession(null);
      setStatisticsTab("tokens");
    });
    void invoke<{ tab?: string | null; sessionId?: string | null } | null>(
      "take_statistics_target",
    )
      .then((target) => {
        if (!target) return;
        if (target.sessionId) applySession(target.sessionId);
        else if (target.tab) setStatisticsTab(target.tab);
      })
      .catch(() => {});
    return () => {
      void pendingSession.then((unlisten) => unlisten());
      void pendingProvider.then((unlisten) => unlisten());
    };
  }, [statistics]);
  const panelServices: PanelServices = {
    sources: snapshot?.sourcePreferences ?? store.sources,
    openSession: async (session) => {
      try {
        await action("open_session", { id: session.id });
        return true;
      } catch {
        return false;
      }
    },
    query: (method, params) => act(method, params),
    openSettingsPage: (page) => {
      if (isTauri()) void invoke("hud_show_main", { page });
    },
    selectSession: (session) => {
      setSelectedSession(session);
      if (isTauri()) {
        void invoke("open_statistics", { sessionId: session.id });
      } else setStatisticsTab("sessions");
    },
    selectProvider: (provider) => {
      const payload = { provider, at: Date.now() };
      try {
        localStorage.setItem(
          storageKeys.highlightProvider,
          JSON.stringify(payload),
        );
      } catch {}
      setSelectedSession(null);
      setStatisticsTab("tokens");
      if (isTauri()) {
        void emit("statistics:provider", payload);
      }
    },
    draft: {
      unsaved: Boolean(
        presentation &&
        (presentation.drafts.app ||
          presentation.drafts.styles.includes(selectedId) ||
          presentation.drafts.layouts.includes(selectedId) ||
          (!presentation.layouts[selectedId] &&
            presentation.drafts.layouts.includes("all"))),
      ),
      layout: !presentation
        ? null
        : presentation.drafts.layouts.includes(selectedId)
          ? "screen"
          : !presentation.layouts[selectedId] &&
              presentation.drafts.layouts.includes("all")
            ? "shared"
            : null,
      style: Boolean(presentation?.drafts.styles.includes(selectedId)),
      app: Boolean(presentation?.drafts.app),
      heldUntil: presentation?.holds[selectedId] ?? null,
      save: () => saveDraft(),
      discard: () => discardDraft(),
      release: () => {
        void releaseHold();
      },
    },
  };
  // Every formatter below takes the interface language and calendar from here.
  const provide = (node: ReactNode) => (
    <FormatProvider
      setting={snapshotLanguage(snapshot)}
      quotaDisplay={snapshot?.settings.quotaDisplay}
    >
      {node}
    </FormatProvider>
  );
  if (surface)
    return provide(
      snapshot && ready && selectedId ? (
        <PanelServicesProvider value={panelServices}>
          <NativeHUDSurface
            glowHidden={glowHidden}
            botDirections={botDirections}
            screenId={selectedId}
            snapshot={snapshot}
            config={config}
            geometry={measured}
            monitors={monitors}
            action={action}
          />
        </PanelServicesProvider>
      ) : null,
    );
  if (statistics)
    return provide(
      <TooltipProvider>
        <div className="statistics-window" data-theme={theme}>
          <header className="statistics-toolbar" data-tauri-drag-region>
            <span
              className="statistics-toolbar-spacer"
              data-tauri-drag-region
            />
            <Tabs value={statisticsTab} onValueChange={setStatisticsTab}>
              <TabsList>
                <TabsTrigger value="tokens">
                  {chinese ? "Token" : "Tokens"}
                </TabsTrigger>
                <TabsTrigger value="sessions">
                  {chinese ? "会话" : "Sessions"}
                </TabsTrigger>
              </TabsList>
            </Tabs>
            <div>{sourceMenu}</div>
          </header>
          {snapshot ? (
            <ViewErrorBoundary
              chinese={chinese}
              onReset={() => setSelectedSession(null)}
            >
              <Suspense fallback={null}>
                <div
                  style={{
                    display: statisticsTab === "tokens" ? "block" : "none",
                  }}
                >
                  <Statistics
                    snapshot={snapshot}
                    light={theme === "light"}
                    action={action}
                    active={windowActive && statisticsTab === "tokens"}
                    onSelectSession={(session) => {
                      setSelectedSession(session);
                      setStatisticsTab("sessions");
                    }}
                  />
                </div>
                <div
                  style={{
                    display: statisticsTab === "sessions" ? "block" : "none",
                  }}
                >
                  <SessionStatistics
                    snapshot={snapshot}
                    light={theme === "light"}
                    action={action}
                    active={windowActive && statisticsTab === "sessions"}
                    initialSessionId={selectedSession?.id ?? null}
                    onClearSelection={() => setSelectedSession(null)}
                  />
                </div>
              </Suspense>
            </ViewErrorBoundary>
          ) : (
            <LoadingState
              label={
                error
                  ? chinese
                    ? "无法读取本机活动"
                    : "Couldn’t read local activity"
                  : chinese
                    ? "正在读取本机活动…"
                    : "Reading local activity…"
              }
              failed={!!error}
              detail={error}
              retryLabel={chinese ? "重试" : "Retry"}
              onRetry={() => void refresh()}
            />
          )}
          <Toaster theme={theme} />
        </div>
      </TooltipProvider>,
    );
  const currentPage = allPages.find((item) => item.id === page);
  const settingsProps = snapshot
    ? { snapshot, config, update: updateConfig, action, active: windowActive }
    : null;
  return provide(
    <PanelServicesProvider value={panelServices}>
      <SettingsNavigation.Provider value={setPage}>
        <TooltipProvider delayDuration={400}>
          <div className="apple-app" data-theme={theme}>
            <aside className="apple-sidebar">
              <div className="sidebar-drag-space" data-tauri-drag-region />
              <nav aria-label={chinese ? "设置" : "Settings"}>
                {pageGroups.map((group) => {
                  const items = allPages.filter((item) => item.group === group);
                  if (!items.length) return null;
                  return (
                    <div className="sidebar-group" key={group}>
                      {items.map((item) => {
                        const { id, label, icon: Icon, color } = item;
                        return (
                          <Button
                            key={id}
                            variant="ghost"
                            className={`apple-navigation ${page === id ? "is-selected" : ""}`}
                            onClick={() => setPage(id)}
                            aria-current={page === id ? "page" : undefined}
                          >
                            <span
                              className="sidebar-symbol"
                              style={{ background: color }}
                            >
                              {item.customIcon ?? (
                                <Icon size={13} strokeWidth={1.9} />
                              )}
                            </span>
                            <span>
                              {chinese
                                ? (chinesePages[id]?.[0] ?? label)
                                : label}
                            </span>
                          </Button>
                        );
                      })}
                    </div>
                  );
                })}
              </nav>
              <div className="sidebar-product">
                <img src={appIcon} alt="Agent HUD" width={32} height={32} />
                <div>
                  <strong>{productName}</strong>
                  <span>
                    {runningVersion} ·{" "}
                    {demo
                      ? chinese
                        ? "演示"
                        : "Demo"
                      : chinese
                        ? "实时"
                        : "Live"}
                  </span>
                </div>
                {sourceMenu}
              </div>
            </aside>
            <main
              className={`apple-content${page === "agents" ? " is-wide" : ""}`}
            >
              <div className="settings-header" data-tauri-drag-region="deep">
                <div className="settings-column">
                  <h1>
                    {chinese
                      ? (chinesePages[page]?.[0] ?? currentPage?.label)
                      : currentPage?.label}
                  </h1>
                  {((chinese ? chinesePages[page]?.[1] : null) ??
                  currentPage?.description) ? (
                    <p>
                      {chinese
                        ? (chinesePages[page]?.[1] ?? currentPage?.description)
                        : currentPage?.description}
                    </p>
                  ) : null}
                </div>
              </div>
              <div className="settings-scroll">
                <div className="settings-pane-content settings-column">
                  {error && (
                    <Alert variant="destructive">
                      <CircleHelp />
                      <AlertTitle>
                        {chinese
                          ? "无法连接数据服务"
                          : "Couldn’t connect to the runtime"}
                      </AlertTitle>
                      <AlertDescription>{error}</AlertDescription>
                    </Alert>
                  )}
                  {!settingsProps ? (
                    <LoadingState
                      label={
                        loading
                          ? chinese
                            ? "正在读取本机活动…"
                            : "Reading local activity…"
                          : chinese
                            ? "数据服务不可用"
                            : "The runtime is unavailable"
                      }
                      failed={!loading}
                      retryLabel={chinese ? "重试" : "Retry"}
                      onRetry={() => void refresh()}
                    />
                  ) : (
                    <Suspense fallback={null}>
                      {extensionPages
                        .find((item) => item.id === page)
                        ?.render({ snapshot, request: extensionRequest })}
                      {(page === "general" ||
                        page === "prices" ||
                        page === "relays" ||
                        page === "local-network" ||
                        page === "display" ||
                        page === "agents" ||
                        page === "agent-plugins" ||
                        page === "updates") && (
                        <SettingsPane
                          page={page}
                          settingsProps={settingsProps}
                          ready={ready}
                          selectedId={selectedId}
                          monitors={monitors}
                          selectScreen={selectScreen}
                          chinese={chinese}
                          layout={config.panelLayout}
                          onLayout={(layout) => setLayout(layout)}
                          preferences={store.sources ?? {}}
                          onPreferences={setSource}
                          agentPlugins={agentPlugins}
                          request={extensionRequest}
                        />
                      )}
                    </Suspense>
                  )}
                </div>
              </div>
            </main>
            <Toaster theme={theme} position="bottom-right" />
          </div>
        </TooltipProvider>
      </SettingsNavigation.Provider>
    </PanelServicesProvider>,
  );
}
