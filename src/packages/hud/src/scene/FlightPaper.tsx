// The paper a flight is drawn with: the envelope (its paper, pocket and
// flap), the plane's line drawing, the letter and the count it carries.
import type { RefObject } from "react";
import { ENVELOPE, PLANE_CREASES, PLANE_POINTS } from "./flight-plan";
import type { Point } from "./flight-route";

/** A still element centred on a point (reduced motion's envelopes). */
export function Placed({
  at,
  refTo,
  size,
  children,
}: {
  at: Point;
  refTo: RefObject<HTMLDivElement | null>;
  size: { width: number; height: number };
  children: React.ReactNode;
}) {
  return (
    <div
      ref={refTo}
      style={{
        position: "absolute",
        left: at.x - size.width / 2,
        top: at.y - size.height / 2,
        width: size.width,
        height: size.height,
        opacity: 0,
      }}
    >
      {children}
    </div>
  );
}

/** An envelope, its flap closed or open. */
export function Envelope({
  open,
  tone,
  count,
}: {
  open: boolean;
  tone: Tone;
  count: number;
}) {
  return (
    <div style={{ position: "absolute", inset: 0 }}>
      <Paper tone={tone}>
        <Pocket ink={tone.ink} />
      </Paper>
      <Flap ink={tone.ink} open={open} />
      {count > 1 && <Count value={count} size={20} />}
    </div>
  );
}

/** The inks a flight draws with. */
export interface Tone {
  ink: string;
  edge: string;
  shade: string;
}

/** A sheet of paper cut to the envelope's (or the plane's) outline: a faint
 * copy a point and a half lower for its shadow, an outline-coloured copy for
 * its edge, and the white paper one point inside it. */
export function Paper({
  tone,
  refs,
  children,
}: {
  tone: Tone;
  refs?: Record<"shade" | "edge" | "body", RefObject<HTMLDivElement | null>>;
  children?: React.ReactNode;
}) {
  const cut = { position: "absolute", clipPath: ENVELOPE } as const;
  return (
    <>
      <div
        ref={refs?.shade}
        style={{
          ...cut,
          inset: 0,
          translate: "0 1.5px",
          background: tone.shade,
        }}
      />
      <div
        ref={refs?.edge}
        style={{ ...cut, inset: 0, background: tone.edge }}
      />
      <div ref={refs?.body} style={{ ...cut, inset: 1, background: "#fff" }}>
        {children}
      </div>
    </>
  );
}

/** The envelope's pocket: the seams from its top corners to the middle. */
export function Pocket({
  ink,
  refTo,
}: {
  ink: string;
  refTo?: RefObject<SVGSVGElement | null>;
}) {
  return (
    <svg
      ref={refTo}
      viewBox="0 0 100 100"
      preserveAspectRatio="none"
      style={{
        position: "absolute",
        inset: 0,
        width: "100%",
        height: "100%",
        overflow: "visible",
      }}
    >
      <path
        d="M0 0L50 60L100 0M0 100L38 46M100 100L62 46"
        fill="none"
        stroke={ink}
        strokeWidth={1}
        vectorEffect="non-scaling-stroke"
        strokeLinejoin="round"
      />
    </svg>
  );
}

/** The envelope's flap: folded down over it, or open above it. */
export function Flap({
  ink,
  open,
  refTo,
}: {
  ink: string;
  open: boolean;
  refTo?: RefObject<SVGSVGElement | null>;
}) {
  return (
    <svg
      ref={refTo}
      viewBox="0 0 100 100"
      preserveAspectRatio="none"
      style={{
        position: "absolute",
        left: 0,
        top: 0,
        width: "100%",
        height: "62%",
        overflow: "visible",
        transformOrigin: "50% 0%",
        scale: open ? "1 -1" : "1 1",
      }}
    >
      <path
        d="M0 0L100 0L50 100Z"
        fill="#f1f2f5"
        stroke={ink}
        strokeWidth={1}
        vectorEffect="non-scaling-stroke"
        strokeLinejoin="round"
      />
    </svg>
  );
}

/** The paper plane's line drawing, shown while it flies: its outline over a
 * copy a point and a half lower for its shadow, and its creases. */
export function Folds({
  tone,
  line,
  height,
  refTo,
}: {
  tone: Tone;
  /** The line's width. */
  line: number;
  /** The craft's height, which the shadow's offset is measured against. */
  height: number;
  refTo?: RefObject<SVGSVGElement | null>;
}) {
  return (
    <svg
      ref={refTo}
      viewBox="0 0 100 100"
      preserveAspectRatio="none"
      style={{
        position: "absolute",
        inset: 0,
        width: "100%",
        height: "100%",
        overflow: "visible",
        opacity: 0,
      }}
    >
      <polygon
        points={PLANE_POINTS}
        fill={tone.shade}
        stroke={tone.shade}
        strokeWidth={line}
        strokeLinejoin="round"
        vectorEffect="non-scaling-stroke"
        transform={`translate(0 ${(150 / height).toFixed(2)})`}
      />
      <polygon
        points={PLANE_POINTS}
        fill="#fff"
        stroke={tone.edge}
        strokeWidth={line}
        strokeLinejoin="round"
        vectorEffect="non-scaling-stroke"
      />
      <path
        d={PLANE_CREASES}
        fill="none"
        stroke={tone.edge}
        strokeWidth={line * 0.85}
        strokeLinecap="round"
        vectorEffect="non-scaling-stroke"
      />
    </svg>
  );
}

/** A sheet of writing paper. */
export function Letter({ tone }: { tone: Tone }) {
  return (
    <div
      style={{
        position: "absolute",
        inset: 0,
        background: "#fff",
        borderRadius: 1.5,
        boxShadow: `0 0 0 1px ${tone.edge}, 0 1.5px 0 1px ${tone.shade}`,
        backgroundImage: `repeating-linear-gradient(to bottom, transparent 0 22%, ${tone.ink} 22% 26%)`,
        backgroundSize: "70% 100%",
        backgroundPosition: "50% 60%",
        backgroundRepeat: "no-repeat",
      }}
    />
  );
}

/** How many flights this plane carries. */
export function Count({ value, size }: { value: number; size: number }) {
  return (
    <span
      style={{
        position: "absolute",
        left: size * 0.2,
        top: -size * 0.5 - 12,
        minWidth: 16,
        height: 16,
        padding: "0 4px",
        boxSizing: "border-box",
        borderRadius: 8,
        background: "#ff453a",
        color: "#fff",
        font: '600 10.5px/16px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif',
        textAlign: "center",
        boxShadow: "0 1px 2px rgba(0,0,0,0.35)",
      }}
    >
      {value}
    </span>
  );
}
