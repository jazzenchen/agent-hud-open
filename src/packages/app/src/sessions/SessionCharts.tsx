import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
  type SVGProps,
} from "react";
import type { StackedBar } from "../lib/session-charts";
import { kindColor, type Theme } from "./SessionTheme";

/** How strongly the sub-agents' part of a bar is drawn (Open's `subagentOpacity`). */
const subagentOpacity = 0.4;
/** Bars grow and shrink, never jump; the window's reduce-motion rule stills them. */
export const grow =
  "y 420ms cubic-bezier(0.22, 1, 0.36, 1), height 420ms cubic-bezier(0.22, 1, 0.36, 1), opacity 160ms ease";
/** The axis the turn chart and the context chart share, so a compaction lines up with its turn. */
export const axis = 44;
export const labelFont = { fontSize: 10, fontFamily: "inherit" } as const;

/** The width an element has, kept as the window resizes. */
export function useWidth<T extends Element>() {
  const [width, setWidth] = useState(0);
  const observer = useRef<ResizeObserver | null>(null);
  const ref = useCallback((node: T | null) => {
    observer.current?.disconnect();
    if (!node) return;
    setWidth(node.getBoundingClientRect().width);
    observer.current = new ResizeObserver(([entry]) =>
      setWidth(entry.contentRect.width),
    );
    observer.current.observe(node);
  }, []);
  useEffect(() => () => observer.current?.disconnect(), []);
  return [ref, width] as const;
}

/** A label that stays readable over lines: a halo in the card's colour behind it. */
export function Halo({
  theme,
  children,
  ...props
}: SVGProps<SVGTextElement> & { theme: Theme; children: ReactNode }) {
  return (
    <text
      {...labelFont}
      paintOrder="stroke"
      stroke={theme.surface}
      strokeWidth={3}
      strokeLinejoin="round"
      {...props}
    >
      {children}
    </text>
  );
}

/**
 * A bar's part, standing on `base`: the first time it is drawn it rises from
 * there (a chart's first bars, a new turn at the end), then moves with `grow`.
 */
export function Rising({
  base,
  y,
  height,
  ...props
}: Omit<SVGProps<SVGRectElement>, "y" | "height" | "style" | "ref"> & {
  base: number;
  y: number;
  height: number;
}) {
  const ref = useRef<SVGRectElement>(null);
  const [risen, setRisen] = useState(false);
  useLayoutEffect(() => {
    // Reading its box takes the part in at its base, so the rise eases.
    ref.current?.getBoundingClientRect();
    setRisen(true);
  }, []);
  return (
    <rect
      ref={ref}
      {...props}
      style={{
        y: risen ? y : base,
        height: risen ? height : 0,
        transition: grow,
      }}
    />
  );
}

/**
 * One stacked bar from `base` up: each part keeps its proportion, a bar that
 * added almost nothing still reaches `least` px, and the sub-agents' parts
 * are faded. With `rise` (the default) a part rises from the base when first
 * drawn; a row's sparkline, one of a hundred, only moves. Returns the rects
 * and where the bar's top is.
 */
export function stackedRects(
  bar: StackedBar,
  x: number,
  width: number,
  base: number,
  scale: number,
  least: number,
  theme: Theme,
  key: string,
  rise = true,
): { rects: ReactNode[]; top: number } {
  if (bar.total <= 0) return { rects: [], top: base };
  const barScale = Math.max(scale, least / bar.total);
  let y = base;
  const rects = bar.parts.map((part, index) => {
    const height = part.value * barScale;
    y -= height;
    const look = {
      x,
      width,
      fill: kindColor(theme, part.kind),
      fillOpacity: part.subagent ? subagentOpacity : 1,
    };
    const place = { y: y + 0.4, height: Math.max(0.6, height - 0.8) };
    return rise ? (
      <Rising key={`${key}-${index}`} {...look} {...place} base={base} />
    ) : (
      <rect
        key={`${key}-${index}`}
        {...look}
        style={{ ...place, transition: grow }}
      />
    );
  });
  return { rects, top: y };
}
