// The HUD's colour tokens, as agent-hud-native-open's Theme and StatusPalette give
// them. A light HUD reads a level from its colour alone, so its fills are
// deepened to about 3:1 and its text to about 4.5:1; on a ground the desktop
// shows through (a solid look with transparency), text takes
// paler shades on dark and deeper ones on light, and the greys become tints
// of the ink (Open's Theme.under).

export type Level = "ok" | "warning" | "critical";
export type AlertTone = "turn" | "calm" | "warning";
/** A mark's level as the HUD's vendors carry it: a quota level, or none. */
export type MarkLevel = "normal" | "warning" | "critical" | "stale";

export interface Ground {
  light: boolean;
  /** A solid look with transparency. */
  seeThrough?: boolean;
}

/** The ground a screen's look gives the HUD drawn on it (Open's
 * HUDLook.seeThrough): a solid surface with a transparency lets the desktop
 * through; an opaque one does not. */
export function lookGround(
  look: { solidTransparency: number },
  light: boolean,
): Ground {
  return { light, seeThrough: look.solidTransparency > 0 };
}

/** The shares of the ink, white on dark and black on light, that the greys
 * take on a see-through ground (Open's Theme.SeeThroughGreys on the HUD's
 * surface): each lays the opaque grey over the look's own colour, so nothing
 * moves as the ground starts to let the desktop through, and a tint always
 * lies between the ground and the ink, so it never runs into either. */
export function seeThroughGreys(ground: Ground) {
  return ground.light
    ? { secondary: 0.55, tertiary: 0.42 }
    : { secondary: 0.6, tertiary: 0.43 };
}

const SURFACE = {
  dark: {
    text: "#f5f5f7",
    secondary: "#98989d",
    tertiary: "#6e6e73",
    divider: "#ffffff1a",
    track: "#ffffff1f",
    card: "#ffffff0d",
    selected: "#ffffff1f",
    // The system blue, which Open's update arrow took as its accent.
    accent: "#0a84ff",
  },
  light: {
    text: "#1c1c1e",
    secondary: "#6e6e73",
    tertiary: "#8e8e93",
    divider: "#00000014",
    track: "#0000001a",
    card: "#0000000a",
    selected: "#0000001f",
    accent: "#007aff",
  },
};

/** The panel's tokens on `ground`. Hairlines, tracks and the chosen chip are
 * tints of the ink already and hold on any ground; on a see-through one the
 * greys become tints too, and a level written as text takes its see-through
 * shade, while its fill keeps its hue. */
export function hudPalette(ground: Ground) {
  const surface = SURFACE[ground.light ? "light" : "dark"];
  const greys = ground.seeThrough ? seeThroughGreys(ground) : null;
  const ink = ground.light ? "0,0,0" : "255,255,255";
  return {
    ...surface,
    ...(greys && {
      secondary: `rgba(${ink},${greys.secondary})`,
      tertiary: `rgba(${ink},${greys.tertiary})`,
    }),
    ok: levelFill("ok", ground),
    warning: levelFill("warning", ground),
    critical: levelFill("critical", ground),
    okText: levelText("ok", ground),
    warningText: levelText("warning", ground),
    criticalText: levelText("critical", ground),
  };
}
export type HUDPalette = ReturnType<typeof hudPalette>;

const LEVEL_FILL = {
  dark: { ok: "#3ddc84", warning: "#ffd23f", critical: "#ff453a" },
  light: { ok: "#1f9d4c", warning: "#c08000", critical: "#ff3b30" },
};
const LEVEL_TEXT_LIGHT = {
  ok: "#1e7e34",
  warning: "#946300",
  critical: "#d70015",
};
const LEVEL_SEE_THROUGH = {
  dark: { ok: "#9df0c0", warning: "#ffe08a", critical: "#ffa39b" },
  light: { ok: "#135e25", warning: "#6e4900", critical: "#a1000f" },
};

/** A level as a fill: a ring, a bar, a dot. */
export function levelFill(level: Level, ground: Ground): string {
  return LEVEL_FILL[ground.light ? "light" : "dark"][level];
}

/** A level as text or a thin glyph. */
export function levelText(level: Level, ground: Ground): string {
  if (ground.seeThrough)
    return LEVEL_SEE_THROUGH[ground.light ? "light" : "dark"][level];
  return ground.light ? LEVEL_TEXT_LIGHT[level] : LEVEL_FILL.dark[level];
}

/** The idle grey as text, for an agent with no level to show (Open's
 * idleText): #9a9aa0 on dark, the light window's secondary on light, and the
 * see-through secondary on a ground the desktop shows through. */
export function idleText(ground: Ground): string {
  if (ground.seeThrough) return hudPalette(ground).secondary;
  return ground.light ? "#6e6e73" : "#9a9aa0";
}

/** A mark's level as text, such as a Simple ring's figure: the level's text
 * shade, or the idle grey's with no level. */
export function markLevelText(level: MarkLevel, ground: Ground): string {
  if (level === "stale") return idleText(ground);
  return levelText(level === "normal" ? "ok" : level, ground);
}

const ALERT_FILL = {
  dark: { turn: "#82aaff", calm: "#6cd8ac", warning: "#e9a16d" },
  light: { turn: "#2f62d0", calm: "#0f7b57", warning: "#a8521c" },
};
const ALERT_SEE_THROUGH = {
  dark: { turn: "#b8cfff", calm: "#a8ecd0", warning: "#f6c9a6" },
  light: { turn: "#1d439c", calm: "#0a5a3f", warning: "#7c3b12" },
};

/** An alert's accent as a fill: a button, a dot, a bar. */
export function alertFill(tone: AlertTone, ground: Ground): string {
  return ALERT_FILL[ground.light ? "light" : "dark"][tone];
}

/** An alert's accent as text or a thin glyph. */
export function alertText(tone: AlertTone, ground: Ground): string {
  if (ground.seeThrough)
    return ALERT_SEE_THROUGH[ground.light ? "light" : "dark"][tone];
  return alertFill(tone, ground);
}

/** Ink on an alert's fill: black on the dark HUD's bright accents, white on
 * a light HUD's deep shades. */
export function onAlertFill(ground: Ground): string {
  return ground.light ? "#ffffff" : "#000000";
}

/** The alerts' ink at `opacity` for captions, account lines and times; a
 * light theme lifts it by 0.2 to stay about as legible as on dark. */
export function alertInk(opacity: number, ground: Ground): string {
  return ground.light
    ? `rgba(29,29,31,${Math.min(1, opacity + 0.2)})`
    : `rgba(255,255,255,${opacity})`;
}

/** A permission card's controls (Open's PermissionColor): Allow is filled
 * green with black ink in every theme; Deny takes the critical level at
 * .14/.3; Always allow is drawn in the ok level. */
export function permissionColors(ground: Ground) {
  const critical = levelFill("critical", ground);
  return {
    allowFill: "#3ddc84",
    onAllow: "#000000",
    denyFill: hexAlpha(critical, 0.14),
    denyEdge: hexAlpha(critical, 0.3),
    denyText: levelText("critical", ground),
    alwaysEdge: levelFill("ok", ground),
    alwaysText: levelText("ok", ground),
  };
}

/** `hex` at `alpha`, for tinted grounds under a token. */
export function hexAlpha(hex: string, alpha: number): string {
  const [r, g, b] = channels(hex);
  return `rgba(${r},${g},${b},${alpha})`;
}

function channels(hex: string): [number, number, number] {
  const value = parseInt(hex.slice(1), 16);
  return [(value >> 16) & 255, (value >> 8) & 255, value & 255];
}
