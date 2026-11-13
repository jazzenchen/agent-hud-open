import {
  lazy,
  Suspense,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { CircleHelp } from "lucide-react";
import { Toaster } from "sonner";
import { Button } from "@agent-hud/ui/components/button";
import { TooltipProvider } from "@agent-hud/ui/components/tooltip";
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@agent-hud/ui/components/alert";
import type {
  PanelLayout,
  Snapshot,
  SourcePreferences,
} from "@agent-hud/protocol";
import appIcon from "../assets/app-icon.png";
import type { NativeHUDMonitor } from "../lib/use-hud-geometry";
import { LoadingState } from "../LoadingState";
import { ViewErrorBoundary } from "../ViewErrorBoundary";
import type { AgentPluginItem } from "./AgentPluginsPane";
import {
  chinesePages,
  pageGroups,
  sidebarPages,
  type AppExtensionPage,
} from "./pages";
import { SettingsNavigation } from "./SettingsNavigation";
import { GuideLayer } from "../guide/GuideLayer";
import type { ProductGuide } from "../guide/plan";
import { openSettingsSteps } from "../guide/steps";
import type { SettingsPaneProps } from "./SettingsPrimitives";

// A resting HUD does not need the settings panes.
const SettingsPane = lazy(() => import("../SettingsPanes"));

type Page = string;

/** The Settings window: the sidebar of pages, and the chosen page's header
 * and pane. */
export function SettingsWindow({
  extensionPages,
  guide,
  agentPlugins,
  productName,
  version,
  theme,
  chinese,
  demo,
  sourceMenu,
  error,
  loading,
  refresh,
  snapshot,
  settingsProps,
  extensionRequest,
  ready,
  selectedId,
  monitors,
  selectScreen,
  layout,
  onLayout,
  preferences,
  onPreferences,
}: {
  extensionPages: AppExtensionPage[];
  guide?: ProductGuide;
  agentPlugins: AgentPluginItem[];
  productName: string;
  version: string;
  theme: "dark" | "light";
  chinese: boolean;
  demo: boolean;
  sourceMenu: ReactNode;
  error: string | null;
  loading: boolean;
  refresh: () => Promise<unknown>;
  snapshot: Snapshot | null;
  settingsProps: SettingsPaneProps | null;
  extensionRequest: (method: string, params?: unknown) => Promise<unknown>;
  ready: boolean;
  selectedId: string;
  monitors: NativeHUDMonitor[];
  selectScreen: (id: string) => void;
  layout: PanelLayout;
  onLayout: (layout: PanelLayout) => Promise<unknown>;
  preferences: Record<string, SourcePreferences>;
  onPreferences: (source: string, patch: SourcePreferences) => Promise<unknown>;
}) {
  const allPages = useMemo(
    () => sidebarPages(extensionPages),
    [extensionPages],
  );
  for (const page of extensionPages)
    if (page.zh) chinesePages[page.id] = page.zh;
  const [page, setPage] = useState<Page>("display");
  // The running app's own version: a release is stamped with its version when
  // it is built (0.6.0-beta.1), which a product's front end cannot know.
  // `version` stands in outside the app; null until the app has said.
  const [runningVersion, setRunningVersion] = useState<string | null>(
    isTauri() ? null : version,
  );
  useEffect(() => {
    if (!isTauri()) return;
    void getVersion().then(setRunningVersion);
  }, []);
  // A page the host asked for (the update notice opens Updates): taken when
  // Settings starts and whenever the host asks again. A product page that
  // takes a built-in page's place answers for it.
  useEffect(() => {
    if (!isTauri()) return;
    const open = (id: string | null) => {
      if (!id) return;
      const target =
        extensionPages.find((page) => page.replaces === id)?.id ?? id;
      if (allPages.some((page) => page.id === target)) setPage(target);
    };
    const take = () =>
      void invoke<string | null>("take_settings_page").then(open);
    take();
    const subscription = getCurrentWebviewWindow().listen<string>(
      "settings:page",
      take,
    );
    return () => void subscription.then((unlisten) => unlisten());
  }, []);
  const currentPage = allPages.find((item) => item.id === page);
  return (
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
                            {chinese ? (chinesePages[id]?.[0] ?? label) : label}
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
                  {runningVersion ?? version} ·{" "}
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
                  // A page that fails to draw shows its error under its own
                  // header; the sidebar stays, and another page clears it.
                  <ViewErrorBoundary key={page} chinese={chinese}>
                    <Suspense fallback={null}>
                      {extensionPages
                        .find((item) => item.id === page)
                        ?.render({ snapshot, request: extensionRequest })}
                      <SettingsPane
                        page={page}
                        settingsProps={settingsProps}
                        ready={ready}
                        selectedId={selectedId}
                        monitors={monitors}
                        selectScreen={selectScreen}
                        chinese={chinese}
                        layout={layout}
                        onLayout={onLayout}
                        preferences={preferences}
                        onPreferences={onPreferences}
                        agentPlugins={agentPlugins}
                        request={extensionRequest}
                      />
                    </Suspense>
                  </ViewErrorBoundary>
                )}
              </div>
            </div>
          </main>
          <GuideLayer
            snapshot={snapshot}
            settingsProps={settingsProps}
            version={runningVersion}
            steps={[...openSettingsSteps, ...(guide?.steps ?? [])]}
            setPage={setPage}
          />
          <Toaster theme={theme} position="bottom-right" />
        </div>
      </TooltipProvider>
    </SettingsNavigation.Provider>
  );
}
