import { FormatProvider } from "@agent-hud/ui/format-context";
import { lazy, useMemo, type ReactNode } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { toast } from "sonner";
import {
  PanelServicesProvider,
  type PanelServices,
} from "@agent-hud/hud/PanelContext";
import { useHUD } from "./lib/use-hud";
import { useWindowActivity } from "./lib/use-window-activity";
import { useBotDirections } from "./lib/use-bot-directions";
import type { HUDScreenContext } from "./lib/hud-config-store";
import { useHUDConfig } from "./lib/use-hud-config";
import { useHUDGeometry } from "./lib/use-hud-geometry";
import { useSelectedScreen } from "./lib/use-selected-screen";
import { useRequestDeliveryToasts } from "./lib/use-request-delivery-toasts";
import { useWindowTheme } from "./lib/use-window-theme";
import { storageKeys } from "./lib/storage-keys";
import { withAppDraft } from "./lib/app-draft";
import { useWebLinks } from "./lib/use-web-links";
import type { HUDConfig } from "@agent-hud/hud/model";
import { snapshotLanguage } from "@agent-hud/ui/format";
import type { AgentPluginItem } from "./settings/AgentPluginsPane";
import type { AppExtensionPage } from "./settings/pages";
import type { ProductGuide } from "./guide/plan";
import { SettingsWindow } from "./settings/SettingsWindow";
import { SourceMenu } from "./SourceMenu";
import { StatisticsWindow } from "./StatisticsWindow";
import { hudEndsLink } from "./lib/hud-ends-link";
import "./lib/animation-handlers";

// Windows load their own views. Statistics does not need the shader/bot renderer,
// and a resting HUD does not need the historical charts and settings panes.
const NativeHUDSurface = lazy(() => import("./HUDSurface"));

export interface AppProps {
  extensionPages?: AppExtensionPage[];
  /** Rows a product build adds to a client's card on Agent plugins. */ agentPlugins?: AgentPluginItem[];
  /** What the product adds to the guide: its news and its Settings steps. */
  guide?: ProductGuide;
  productName?: string;
  version?: string;
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
  guide,
  productName = "Agent HUD",
  version = "0.1.0",
}: AppProps) {
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
  const { selectedId, selectScreen } = useSelectedScreen(
    surfaceScreenId,
    monitors,
    settings,
  );
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
  const snapshot = useMemo(
    () => withAppDraft(collectedSnapshot, store.appDraft),
    [collectedSnapshot, store.appDraft, store.sources],
  );
  useRequestDeliveryToasts(snapshot, settings && windowActive);
  useWebLinks((url) => act("open_link", { url }));
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
  const hudVisible = snapshot?.settings.hudVisible !== false;
  const theme = useWindowTheme(snapshot, store, monitors, surface);
  const chinese = snapshotLanguage(snapshot) === "zh";
  const sourceMenu = (
    <SourceMenu
      chinese={chinese}
      statistics={statistics}
      hudVisible={hudVisible}
      act={act}
    />
  );
  const panelServices: PanelServices = {
    sources: snapshot?.sourcePreferences ?? store.sources,
    ends: surface ? hudEndsLink : undefined,
    openSession: async (session) => {
      try {
        await act("open_session", { id: session.id });
        return true;
      } catch {
        return false;
      }
    },
    query: (method, params) => act(method, params),
    openSettingsPage: (page) => {
      if (isTauri()) void invoke("hud_show_main", { page });
    },
    openClient: (source) => {
      void act("open_client", { source }).catch((failure) =>
        toast.error(String(failure)),
      );
    },
    webSignIn: (provider, host) => {
      // The HUD has no place for a failure, so it shows as a toast.
      if (isTauri())
        void invoke("web_sign_in", { provider, host }).catch((failure) =>
          toast.error(String(failure)),
        );
    },
    // The Statistics window keeps its own tab and session; the HUD and
    // Settings only ask it to show one.
    selectSession: (session) => {
      if (isTauri()) {
        void invoke("open_statistics", { sessionId: session.id });
      }
    },
    selectProvider: (provider) => {
      const payload = { provider, at: Date.now() };
      localStorage.setItem(
        storageKeys.highlightProvider,
        JSON.stringify(payload),
      );
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
            action={act}
            updateConfig={updateConfig}
            demo={demo}
            guide={guide}
          />
        </PanelServicesProvider>
      ) : null,
    );
  if (statistics)
    return provide(
      <StatisticsWindow
        snapshot={snapshot}
        theme={theme}
        chinese={chinese}
        act={act}
        windowActive={windowActive}
        demo={demo}
        error={error}
        refresh={refresh}
        sourceMenu={sourceMenu}
      />,
    );
  const settingsProps = snapshot
    ? {
        snapshot,
        config,
        update: updateConfig,
        action: act,
        active: windowActive,
      }
    : null;
  return provide(
    <PanelServicesProvider value={panelServices}>
      <SettingsWindow
        extensionPages={extensionPages}
        guide={guide}
        agentPlugins={agentPlugins}
        productName={productName}
        version={version}
        theme={theme}
        chinese={chinese}
        demo={demo}
        sourceMenu={sourceMenu}
        error={error}
        loading={loading}
        refresh={refresh}
        snapshot={snapshot}
        settingsProps={settingsProps}
        extensionRequest={extensionRequest}
        ready={ready}
        selectedId={selectedId}
        monitors={monitors}
        selectScreen={selectScreen}
        layout={config.panelLayout}
        onLayout={(layout) => setLayout(layout)}
        preferences={store.sources ?? {}}
        onPreferences={setSource}
      />
    </PanelServicesProvider>,
  );
}
