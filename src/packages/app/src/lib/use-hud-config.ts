import { useCallback, useEffect, useRef, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import type { PanelLayout, SourcePreferences } from "@agent-hud/protocol";
import type { HUDConfig } from "@agent-hud/hud/model";
import {
  getScreenHUDConfig,
  migrateHUDConfigStore,
  parseHUDConfigStore,
  updateScreenHUDConfig,
  type HUDConfigStore,
  type HUDScreenContext,
} from "./hud-config-store";
import { readJSON, storageKeys } from "./storage-keys";

const configKey = storageKeys.hudConfig;
const previousKey = storageKeys.hudConfigV2;
const legacyKey = storageKeys.hudConfigV1;

export interface PresentationState {
  initialized: boolean;
  revision: number;
  store: HUDConfigStore;
  /** The menu bar's Hide notch glow, for every screen. */
  glowHidden: boolean;
  sharedLayout: PanelLayout;
  layouts: Record<string, PanelLayout>;
  sources: Record<string, SourcePreferences>;
  appDraft: Record<string, any> | null;
  drafts: { layouts: string[]; styles: string[]; app: boolean };
  holds: Record<string, number>;
}
export async function presentationRequest(
  method: string,
  params: unknown = {},
) {
  return invoke<PresentationState>("presentation_request", { method, params });
}
export function presentationStore(state: PresentationState): HUDConfigStore {
  const global = { ...state.store.global };
  const app = state.appDraft;
  if (app) {
    if (typeof app.botLogos === "boolean") global.botLogos = app.botLogos;
    for (const [key, target] of [
      ["delayMs", "hoverDelayMs"],
      ["collapseDelayMs", "collapseDelayMs"],
      ["pressOptionToOpen", "requiresOptionToOpen"],
      ["openAtTopEdge", "openImmediatelyAtTop"],
    ] as const) {
      if (app.hover?.[key] !== undefined)
        (global as Record<string, unknown>)[target] = app.hover[key];
    }
  }
  return {
    ...state.store,
    global,
    sharedLayout: state.sharedLayout,
    layouts: state.layouts,
    sources: state.sources,
    appDraft: state.appDraft,
  };
}
function readStore() {
  return parseHUDConfigStore(readJSON(configKey));
}
function publish(store: HUDConfigStore) {
  if (isTauri())
    void emit("hud:config", store).catch((error) =>
      console.error("Display preference synchronization failed", error),
    );
}
function initializeStore(screens: HUDScreenContext[]): HUDConfigStore | null {
  const existing = readStore();
  if (existing) return existing;
  // Connected screen identities are required before copying the former shared values.
  if (!screens.length) return null;
  const store = migrateHUDConfigStore(
    readJSON(previousKey),
    readJSON(legacyKey),
    screens,
  );
  localStorage.setItem(configKey, JSON.stringify(store));
  publish(store);
  return store;
}
/** `active` is whether the window is on screen; it reads the preferences again when it is back. */
export function useHUDConfig(
  selectedId: string,
  screens: HUDScreenContext[],
  active = true,
) {
  const contexts = useRef(screens);
  contexts.current = screens;
  const [loaded, setLoaded] = useState(() => {
    const store = isTauri() ? null : readStore();
    return {
      store:
        store ??
        ({ version: 3, global: {}, screens: {} } satisfies HUDConfigStore),
      ready: store !== null,
    };
  });
  const [presentation, setPresentation] = useState<PresentationState | null>(
    null,
  );
  useEffect(() => {
    if (!isTauri()) {
      const store = initializeStore(screens);
      if (store) setLoaded({ store, ready: true });
      return;
    }
    if (!active) return;
    let disposed = false,
      latest = -1;
    // A state read while an event was on its way is older than the event.
    const receive = (state: PresentationState) => {
      if (disposed || state.revision < latest) return;
      latest = state.revision;
      setPresentation(state);
      setLoaded({ store: presentationStore(state), ready: state.initialized });
    };
    const subscription = getCurrentWebviewWindow().listen<PresentationState>(
      "hud:presentation",
      (event) => receive(event.payload),
    );
    void subscription
      .then(() => presentationRequest("state"))
      .then(async (state) => {
        if (!state.initialized && screens.length) {
          const legacy =
            readStore() ??
            migrateHUDConfigStore(
              readJSON(previousKey),
              readJSON(legacyKey),
              screens,
            );
          state = await presentationRequest("import", { store: legacy });
        }
        receive(state);
      })
      .catch((error) =>
        console.error("Display preferences could not be read", error),
      );
    return () => {
      disposed = true;
      void subscription.then((unlisten) => unlisten());
    };
  }, [screens, active]);
  useEffect(() => {
    if (isTauri()) return;
    const receive = () => {
      const store = readStore();
      if (store) setLoaded({ store, ready: true });
    };
    const changed = (event: StorageEvent) => {
      if (event.key === configKey || event.key === null) receive();
    };
    window.addEventListener("storage", changed);
    return () => window.removeEventListener("storage", changed);
  }, []);
  const updateScreen = useCallback((id: string, patch: Partial<HUDConfig>) => {
    if (isTauri()) {
      void presentationRequest("update", { screen: id, patch })
        .then((state) => {
          setPresentation(state);
          setLoaded({
            store: presentationStore(state),
            ready: state.initialized,
          });
        })
        .catch((error) =>
          console.error("Display preferences were not saved", error),
        );
      return;
    }
    const latest = readStore() ?? initializeStore(contexts.current);
    if (!latest || !id)
      throw new Error(
        "Display preferences are not ready until screens are known",
      );
    const next = updateScreenHUDConfig(latest, id, patch);
    localStorage.setItem(configKey, JSON.stringify(next));
    setLoaded({ store: next, ready: true });
    publish(next);
  }, []);
  const update = useCallback(
    (patch: Partial<HUDConfig>) => updateScreen(selectedId, patch),
    [selectedId, updateScreen],
  );
  const selected = screens.find((screen) => screen.id === selectedId) ?? {
    id: selectedId || "preview",
    monitorIndex: 0,
    hasNotch: false,
  };
  return {
    config: getScreenHUDConfig(loaded.store, selected),
    store: loaded.store,
    ready: loaded.ready,
    update,
    updateScreen,
    presentation,
    setLayout: async (layout: PanelLayout, screen = selectedId) =>
      presentationRequest("layout", { screen, layout }),
    setSource: async (source: string, patch: SourcePreferences) =>
      presentationRequest("source_update", { source, patch }),
    saveDraft: async (screen = selectedId) =>
      presentationRequest("save", { screen }),
    discardDraft: async (screen = selectedId) =>
      presentationRequest("discard", { screen }),
    releaseHold: async (screen = selectedId) =>
      presentationRequest("release", { screen }),
  };
}
