import { memo, useLayoutEffect, useMemo, useRef, type ReactNode } from "react";
import type { Snapshot } from "@agent-hud/protocol";
import { PanelServicesProvider, type PanelServices } from "../PanelContext";
import {
  ListedSessionsContext,
  type ListedSessions,
} from "../panel/listed-sessions";

/** `value` while `live`; otherwise the value it had when it was last live. */
export function useLatestWhile<T>(value: T, live: boolean): T {
  const latest = useRef(value);
  if (live) latest.current = value;
  return latest.current;
}

/** What the measuring copy draws: the snapshot and services as they were
 * while `live` last held, taken again whenever one of `shown` changes. */
export function useMeasured(
  snapshot: Snapshot,
  services: PanelServices | null,
  listed: ListedSessions,
  live: boolean,
  shown: unknown[],
) {
  const shownSnapshot = useLatestWhile(snapshot, live);
  const shownServices = useLatestWhile(services, live);
  return useMemo(
    () => ({ snapshot: shownSnapshot, services: shownServices, listed }),
    [shownSnapshot, shownServices, ...shown],
  );
}

/** Draws again only when `version` changes; between, React keeps what it drew. */
const Redraw = memo(
  function Redraw({ draw }: { version: unknown; draw: () => ReactNode }) {
    return <>{draw()}</>;
  },
  (previous, next) => previous.version === next.version,
);

/** The panel's body, drawn out of sight at its natural height for the panel's
 * next opening; it draws again only when `measured` changes. */
export function MeasuringCopy({
  measured,
  body,
  width,
  padding,
  onHeight,
}: {
  measured: {
    snapshot: Snapshot;
    services: PanelServices | null;
    listed: ListedSessions;
  };
  body: (shown: Snapshot) => ReactNode;
  width: number;
  padding: { top: number; right: number; bottom: number; left: number };
  onHeight: (height: number) => void;
}) {
  const content = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const element = content.current;
    if (!element) return;
    const measure = (height: number) => {
      const naturalHeight = Math.ceil(height);
      if (naturalHeight > 0) onHeight(naturalHeight);
    };
    // Measure the same real content before the first hover, as the native hosting view does.
    measure(element.getBoundingClientRect().height);
    const observer = new ResizeObserver((entries) =>
      measure(
        entries[0].borderBoxSize?.[0]?.blockSize ??
          element.getBoundingClientRect().height,
      ),
    );
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  return (
    <div
      ref={content}
      aria-hidden="true"
      inert
      data-hud-measurement
      style={{
        position: "absolute",
        left: 0,
        top: 0,
        width,
        visibility: "hidden",
        pointerEvents: "none",
        paddingTop: padding.top,
        paddingRight: padding.right,
        paddingBottom: padding.bottom,
        paddingLeft: padding.left,
      }}
    >
      <Redraw
        version={measured}
        draw={() => {
          const drawn = (
            <ListedSessionsContext.Provider value={measured.listed}>
              {body(measured.snapshot)}
            </ListedSessionsContext.Provider>
          );
          return measured.services ? (
            <PanelServicesProvider value={measured.services}>
              {drawn}
            </PanelServicesProvider>
          ) : (
            drawn
          );
        }}
      />
    </div>
  );
}
