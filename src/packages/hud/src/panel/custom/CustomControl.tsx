/* A control an agent added to the panel, drawn from its node in the layout's
 * `custom` with the panel's own parts: its type and fonts, the level and agent
 * colours, the rings and the token chart. Its clicks go through the panel's own
 * services, so opening Statistics or a session behaves as the panel's rows do. */
import { useEffect, useMemo, useState, type ReactNode } from "react";
import type { PanelNode, Snapshot } from "@agent-hud/protocol";
import { useFormat } from "@agent-hud/ui/format-context";
import { useGround } from "../../ground";
import { palette, type Logo } from "../theme";
import {
  collections,
  initialState,
  readsTime,
  type Scope,
} from "./custom-data";
import { Placed } from "./Control";
import { DrawingContext, type Drawing, type Node } from "./drawing";

/** The time, drawn again as each minute turns while the panel is open. */
function useMinute(enabled: boolean, start: number) {
  const [now, setNow] = useState(() => (enabled ? Date.now() : start));
  useEffect(() => {
    if (!enabled) return;
    setNow(Date.now());
    let timer: ReturnType<typeof setTimeout>;
    const tick = () => {
      setNow(Date.now());
      timer = setTimeout(tick, 60_000 - (Date.now() % 60_000) + 50);
    };
    timer = setTimeout(tick, 60_000 - (Date.now() % 60_000) + 50);
    return () => clearTimeout(timer);
  }, [enabled]);
  return enabled ? now : start;
}

/** A control placed in the panel or among its blocks, with the states it declares. */
export function CustomControl({
  node,
  snapshot,
  light,
  Logo,
  active,
  showReset,
  onStats,
  divided,
  section,
  separated = true,
}: {
  node: PanelNode;
  snapshot: Snapshot;
  light: boolean;
  Logo: Logo;
  active: boolean;
  showReset: boolean;
  onStats?: () => void;
  /** A line above it, as the panel's other sections draw one. */
  divided?: boolean;
  /** Draws one of the panel's own sections that a row, column or grid holds. */
  section?: (
    key: string,
    width: number | undefined,
    bare: boolean,
  ) => ReactNode;
  /** The panel draws lines between its parts. */
  separated?: boolean;
}) {
  const colors = palette(useGround(light));
  const format = useFormat();
  const root = node as Node;
  const [state, setStates] = useState(() => initialState(root as never));
  const all = useMemo(() => collections(snapshot), [snapshot]);
  const now = useMinute(
    active && readsTime(node),
    Date.parse(snapshot.generatedAt),
  );
  const drawing: Drawing = {
    all,
    snapshot,
    colors,
    light,
    Logo,
    active,
    showReset,
    onStats,
    root,
    section,
    separated,
    setState: (name, value) =>
      value === undefined
        ? undefined
        : setStates((current) => ({ ...current, [name]: value })),
  };
  const scope: Scope = {
    items: [],
    state,
    now,
    language: format.language,
    calendar: format.calendar,
  };
  return (
    <DrawingContext.Provider value={drawing}>
      <div
        data-hud-custom={node.id}
        style={{
          color: colors.text,
          minWidth: 0,
          ...(divided
            ? {
                boxShadow: `inset 0 1px 0 ${colors.divider}`,
                paddingTop: "var(--hud-divided-gap)",
              }
            : {}),
        }}
      >
        <Placed node={root} scope={scope} parent="panel" index={0} />
      </div>
    </DrawingContext.Provider>
  );
}
