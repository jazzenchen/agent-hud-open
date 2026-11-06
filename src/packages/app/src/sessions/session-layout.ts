/** What a session's detail has open beside it: a turn of its own, or one of
 * its sub-agents' sessions. */
export type Pick = { kind: "turn" | "session"; id: string };

/** How the Sessions page sets out its columns at a width: one column that
 * drills in; the list beside a session (a pick then takes the list's place);
 * or the list, the session and the pick side by side. */
export type SessionLayout = "one" | "two" | "three";

export type SessionColumn = "list" | "session" | "pick";

/** Below this page width, one column at a time. */
export const splitWidth = 1000;
/** The page's side left of its first column. */
const side = 22;
/** The session's column never narrows below 480 points of content inside
 * its 22-point sides. */
const sessionMin = 524;

const clamp = (value: number, low: number, high: number) =>
  Math.min(high, Math.max(low, Math.round(value)));

/** The pick's column: a turn's calls or a sub-agent's page. */
export const pickWidth = (width: number) => clamp(width * 0.3, 420, 480);

/** The list's column beside a session and a pick; beside a session alone it
 * keeps its own wider share. */
export const listWidth = (width: number) => clamp(width * 0.32, 440, 520);

/** The layout for the page's whole width. */
export function sessionLayout(width: number): SessionLayout {
  if (width < splitWidth) return "one";
  return width - side - listWidth(width) - pickWidth(width) >= sessionMin
    ? "three"
    : "two";
}

/** The columns shown at the page's whole width, left to right, the grid's
 * tracks for them, and how wide the list's rows are (0 while it is hidden). */
export function sessionColumns(
  width: number,
  session: boolean,
  pick: boolean,
): {
  layout: SessionLayout;
  shown: SessionColumn[];
  tracks: string;
  list: number;
} {
  const layout = sessionLayout(width);
  const picked = session && pick;
  if (layout === "one") {
    const shown: SessionColumn = picked ? "pick" : session ? "session" : "list";
    return {
      layout,
      shown: [shown],
      tracks: "minmax(0, 1fr)",
      list: shown === "list" ? width - 2 * side : 0,
    };
  }
  // The list's column keeps 16 points right of its rows.
  if (!picked)
    return {
      layout,
      shown: ["list", "session"],
      tracks: "clamp(440px, 42%, 580px) minmax(0, 1fr)",
      list: clamp((width - side) * 0.42, 440, 580) - 16,
    };
  return layout === "two"
    ? {
        layout,
        shown: ["session", "pick"],
        tracks: `minmax(0, 1fr) ${pickWidth(width)}px`,
        list: 0,
      }
    : {
        layout,
        shown: ["list", "session", "pick"],
        tracks: `${listWidth(width)}px minmax(0, 1fr) ${pickWidth(width)}px`,
        list: listWidth(width) - 16,
      };
}
