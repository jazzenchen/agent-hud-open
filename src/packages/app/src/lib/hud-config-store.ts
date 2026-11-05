// Display preferences as they are kept: global settings, each screen's own,
// and its panel layout. Reads, migrates and updates them, and works out one
// screen's HUD configuration from them.
import type { PanelLayout, SourcePreferences } from "@agent-hud/protocol";
import { defaultHUDConfig, type HUDConfig } from "@agent-hud/hud/model";
import { usageStyle } from "@agent-hud/hud/usage-meter/arcs";

export interface HUDScreenContext {
  id: string;
  monitorIndex: number;
  hasNotch: boolean;
}
type HardwareField = "monitorIndex" | "hasNotch" | "notchWidth" | "notchHeight";
// IslandPane's behavior and Bot logos use global Settings bindings. Expanded-panel
// switches and order belong to the selected screen's layout (PanelLayoutStore).
const globalKeys = [
  "botLogos",
  "hoverDelayMs",
  "collapseDelayMs",
  "messageFadeSeconds",
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
  const settings = pickSettings(value, settingKeys);
  // `simpleStyle` took over from `showsRings`: a screen saved with its rings
  // off draws the number alone (plans.rs reads it the same way).
  if (!usageStyle(settings.simpleStyle)) {
    delete settings.simpleStyle;
    if (record(value) && value.showsRings === false)
      settings.simpleStyle = "numbers";
  }
  return settings;
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
              ...defaultHUDConfig.panelSections,
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
