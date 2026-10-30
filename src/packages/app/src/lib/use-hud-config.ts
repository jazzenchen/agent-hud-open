import { useCallback, useEffect, useRef, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import type { PanelLayout, SourcePreferences } from "@agent-hud/protocol";
import { defaultHUDConfig, type HUDConfig } from "@agent-hud/hud/model";
import { storageKeys } from "./storage-keys";
const configKey = storageKeys.hudConfig;
const previousKey = storageKeys.hudConfigV2;
const legacyKey = storageKeys.hudConfigV1;

export interface HUDScreenContext {
  id: string;
  monitorIndex: number;
  hasNotch: boolean;
}
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
type HardwareField = "monitorIndex" | "hasNotch" | "notchWidth" | "notchHeight";
// IslandPane's behavior and Bot logos use global Settings bindings. Expanded-panel
// switches and order belong to the selected screen's layout (PanelLayoutStore).
const globalKeys = [
  "botLogos",
  "hoverDelayMs",
  "collapseDelayMs",
  "requiresOptionToOpen",
  "openImmediatelyAtTop",
  "animations",
] as const satisfies readonly (keyof HUDConfig)[];
const oldSharedPanelKeys = [
  "sessionList",
  "quotaDetails",
  "resetCountdown",
  "tokenUsage",
  "panelWidth",
] as const satisfies readonly (keyof HUDConfig)[];
type GlobalField = (typeof globalKeys)[number];
export type HUDGlobalSettings = Partial<Pick<HUDConfig, GlobalField>>;
export type HUDScreenSettings = Partial<
  Omit<HUDConfig, HardwareField | GlobalField>
>;
export interface HUDConfigStore {
  version: 3;
  global: HUDGlobalSettings;
  screens: Record<string, HUDScreenSettings>;
  screenDefaults?: HUDScreenSettings;
  sharedLayout?: PanelLayout;
  layouts?: Record<string, PanelLayout>;
  sources?: Record<string, SourcePreferences>;
  appDraft?: Record<string, unknown> | null;
}
const hardwareFields = new Set<string>([
  "monitorIndex",
  "hasNotch",
  "notchWidth",
  "notchHeight",
]);
const settingKeys = Object.keys(defaultHUDConfig).filter(
  (key) =>
    !hardwareFields.has(key) && !globalKeys.some((global) => global === key),
) as (keyof HUDScreenSettings)[];
function record(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
function stringList(value: unknown): string[] | null {
  return Array.isArray(value)
    ? [
        ...new Set(
          value.filter(
            (item): item is string =>
              typeof item === "string" && item.length > 0,
          ),
        ),
      ]
    : null;
}
/** One projection for persisted documents, migration and updates; no runtime hardware is saved. */
function pickSettings<K extends keyof HUDConfig>(
  value: unknown,
  keys: readonly K[],
): Partial<Pick<HUDConfig, K>> {
  if (!record(value)) return {};
  return Object.fromEntries(
    keys.flatMap((key) => {
      let setting = value[key];
      const fallback = defaultHUDConfig[key];
      if (Array.isArray(fallback)) {
        const values = stringList(setting);
        if (!values) return [];
        // Unknown sections are omitted; unlisted sections retain the source's default order.
        setting =
          key === "panelSections"
            ? [
                ...values.filter((item) =>
                  (fallback as string[]).includes(item),
                ),
                ...fallback.filter((item) => !values.includes(item)),
              ]
            : values;
      } else if (record(fallback)) {
        if (!record(setting)) return [];
        setting = { ...setting };
      } else if (
        typeof setting !== typeof fallback ||
        (typeof setting === "number" && !Number.isFinite(setting))
      )
        return [];
      return [[key, setting]];
    }),
  ) as Partial<Pick<HUDConfig, K>>;
}
function screenSettings(value: unknown): HUDScreenSettings {
  return pickSettings(value, settingKeys);
}
export function parseHUDConfigStore(value: unknown): HUDConfigStore | null {
  if (!record(value) || value.version !== 3 || !record(value.screens))
    return null;
  return {
    version: 3,
    global: pickSettings(value.global, globalKeys),
    screens: Object.fromEntries(
      Object.entries(value.screens).map(([id, settings]) => [
        id,
        screenSettings(settings),
      ]),
    ),
  };
}
/** Only data formats actually written by this app are carried. Old shared panel values
 * are copied once to screens that already drew them; subsequent edits are independent. */
export function migrateHUDConfigStore(
  previous: unknown,
  legacy: unknown,
  screens: HUDScreenContext[],
): HUDConfigStore {
  if (record(previous) && previous.version === 2 && record(previous.screens)) {
    const previousScreens = previous.screens;
    const sourceGlobal = record(previous.global)
      ? previous.global
      : Object.values(previousScreens)[0];
    const sharedPanel = record(previous.global)
      ? pickSettings(previous.global, oldSharedPanelKeys)
      : {};
    const ids = new Set([
      ...Object.keys(previousScreens),
      ...screens.map((screen) => screen.id),
    ]);
    return {
      version: 3,
      global: pickSettings(sourceGlobal, globalKeys),
      screens: Object.fromEntries(
        [...ids].map((id) => [
          id,
          { ...screenSettings(previousScreens[id]), ...sharedPanel },
        ]),
      ),
    };
  }
  const previousIndex =
    record(legacy) && Number.isInteger(legacy.monitorIndex)
      ? legacy.monitorIndex
      : 0;
  const previousScreen = screens.find(
    (screen) => screen.monitorIndex === previousIndex,
  );
  return {
    version: 3,
    global: pickSettings(legacy, globalKeys),
    screens:
      record(legacy) && previousScreen
        ? { [previousScreen.id]: screenSettings(legacy) }
        : {},
  };
}
function readJSON(key: string): unknown {
  try {
    return JSON.parse(localStorage.getItem(key) || "null");
  } catch {
    return null;
  }
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
export function getScreenHUDConfig(
  store: HUDConfigStore,
  screen: HUDScreenContext,
): HUDConfig {
  const layout = store.layouts?.[screen.id] ?? store.sharedLayout ?? {};
  const hidden = new Set(layout.hiddenSections ?? []);
  return {
    ...defaultHUDConfig,
    mode: screen.hasNotch ? "notch" : "logos",
    ...screenSettings(store.screenDefaults),
    ...screenSettings(store.screens[screen.id]),
    ...pickSettings(store.global, globalKeys),
    monitorIndex: screen.monitorIndex,
    hasNotch: screen.hasNotch,
    panelLayout: layout,
    ...(store.layouts || store.sharedLayout
      ? {
          sessionList: !hidden.has("sessions"),
          quotaDetails: !hidden.has("blocks"),
          tokenUsage: !hidden.has("tokens"),
          resetCountdown: layout.countdown ?? true,
          panelWidth: layout.style?.width ?? 540,
          panelOrder: layout.order ?? [],
          panelSections: [
            ...new Set([
              ...(layout.sections ?? []),
              "blocks",
              "tokens",
              "sessions",
            ]),
          ] as HUDConfig["panelSections"],
          layoutDensity:
            layout.style?.spacing === "compact"
              ? "compact"
              : layout.style?.spacing === "roomy"
                ? "comfortable"
                : "normal",
        }
      : {}),
  };
}
/** This is the only update reducer; a screen edit cannot write another screen's layout. */
export function updateScreenHUDConfig(
  store: HUDConfigStore,
  id: string,
  patch: Partial<HUDConfig>,
): HUDConfigStore {
  return {
    version: 3,
    global: { ...store.global, ...pickSettings(patch, globalKeys) },
    screens: {
      ...store.screens,
      [id]: { ...store.screens[id], ...screenSettings(patch) },
    },
  };
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
