// The guide's small window beside the resting HUD: where the HUD's own
// cards come out (below a notch or a top strip, inward from a side or the
// bottom), in the HUD's own colours and corners. Its content is the host's,
// and so is its width. It is read over whatever window lies behind it, so
// its ground stays solid whatever the HUD's transparency. It steps aside
// while the panel is open or another card is out, and keeps its content
// alive meanwhile. Clicks reach it as the HUD's own (`.hud-guide` is a hit
// region); the host keeps it from opening the panel.
import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
} from "react";
import type { HUDRect } from "./geometry";
import { HUD_METRICS, type HUDEdge } from "./model";
import { hudGround } from "./scene/useHUDLayout";

export interface HUDGuide {
  content: ReactNode;
  /** The window's width; a step of the tour is 272. */
  width?: number;
  /** The usage panel opened (true) or closed. */
  onPanel?: (open: boolean) => void;
}

const stepWidth = 272;
/** The gap a card keeps from the strip it comes out of (cardRect). */
const gap = 8;
const margin = 8;

function place(
  edge: HUDEdge,
  strip: HUDRect,
  screen: { width: number; height: number },
  width: number,
  height: number,
): CSSProperties {
  const left = Math.min(
    Math.max(margin, strip.x + strip.width / 2 - width / 2),
    screen.width - width - margin,
  );
  const top = Math.min(
    Math.max(margin, strip.y + strip.height / 2 - height / 2),
    screen.height - height - margin,
  );
  if (edge === "top") return { left, top: strip.y + strip.height + gap };
  if (edge === "bottom") return { left, bottom: screen.height - strip.y + gap };
  if (edge === "left") return { left: strip.x + strip.width + gap, top };
  return { right: screen.width - strip.x + gap, top };
}

const origins: Record<HUDEdge, string> = {
  top: "top center",
  bottom: "bottom center",
  left: "center left",
  right: "center right",
};

export function GuideDock({
  guide,
  open,
  hidden,
  edge,
  strip,
  screen,
  light,
  reduceMotion,
}: {
  guide: HUDGuide | null | undefined;
  open: boolean;
  /** Something else is out (an alert card, a drag): the window waits. */
  hidden: boolean;
  edge: HUDEdge;
  strip: HUDRect;
  screen: { width: number; height: number };
  /** The HUD's appearance is light. */
  light: boolean;
  reduceMotion: boolean;
}) {
  const box = useRef<HTMLDivElement>(null);
  const [height, setHeight] = useState(120);
  const onPanel = useRef(guide?.onPanel);
  onPanel.current = guide?.onPanel;
  useEffect(() => onPanel.current?.(open), [open]);
  const present = Boolean(guide);
  useLayoutEffect(() => {
    const element = box.current;
    if (!element) return;
    // A hidden window measures 0; it keeps the height it last had.
    const measure = () => {
      if (element.offsetHeight > 0) setHeight(element.offsetHeight);
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, [present]);
  if (!guide) return null;
  const shown = !open && !hidden;
  const width = guide.width ?? stepWidth;
  const ground = hudGround(light, 1);
  return (
    <div
      ref={box}
      className="hud-guide"
      data-theme={light ? "light" : "dark"}
      style={{
        position: "absolute",
        ...place(edge, strip, screen, width, height),
        width,
        zIndex: 30,
        display: shown ? "block" : "none",
        boxSizing: "border-box",
        padding: "13px 14px 14px",
        borderRadius: HUD_METRICS.cardRadius,
        background: ground.fill,
        border: `1px solid ${ground.outline}`,
        boxShadow: ground.shadow,
        color: light ? "#1c1c1e" : "#f2f2f7",
        fontSize: 12.5,
        lineHeight: 1.45,
        transformOrigin: origins[edge],
        animation: reduceMotion
          ? undefined
          : "hud-guide-in 0.32s cubic-bezier(0.2, 0.9, 0.3, 1.15)",
      }}
    >
      <style>
        {
          "@keyframes hud-guide-in { from { opacity: 0; transform: scale(0.94); } } @media (prefers-reduced-motion: reduce) { .hud-guide * { animation: none !important; } }"
        }
      </style>
      {guide.content}
    </div>
  );
}
