import { useEffect, useLayoutEffect, useRef } from "react";
import { motion } from "motion/react";
import { Hand, MessageSquareText } from "lucide-react";
import type { HUDRect } from "../geometry";
import { HUD_METRICS, type HUDMode } from "../model";
import { useGround } from "../ground";
import { alertInk, alertText } from "../tokens";
import { ringPath, type EndSide } from "./end-geometry";
import type { EndFade } from "./useParkedEnds";

const box = (rect: HUDRect) => ({
  left: rect.x,
  top: rect.y,
  width: rect.width,
  height: rect.height,
});

/** The tail's ring: full when a round starts, empty when its oldest message
 * goes. It stops with the round's clock. */
function FadeRing({
  width,
  height,
  fade,
  color,
  track,
}: {
  width: number;
  height: number;
  fade: EndFade;
  color: string;
  track: string;
}) {
  const ring = ringPath(width, height, 0.9);
  const line = useRef<SVGPathElement>(null);
  const run = useRef<Animation | null>(null);
  // Where the round was, so a ring redrawn for a wider pill goes on from there.
  const kept = useRef({ cycle: fade.cycle, time: 0 });
  useLayoutEffect(() => {
    const element = line.current;
    if (!element?.animate) return;
    if (kept.current.cycle !== fade.cycle)
      kept.current = { cycle: fade.cycle, time: 0 };
    const animation = element.animate(
      [{ strokeDashoffset: 0 }, { strokeDashoffset: ring.length }],
      { duration: fade.ms, easing: "linear", fill: "forwards" },
    );
    animation.currentTime = kept.current.time;
    if (fade.paused) animation.pause();
    run.current = animation;
    return () => {
      kept.current.time = Number(animation.currentTime ?? 0);
      animation.cancel();
    };
  }, [fade.cycle, ring.length, fade.ms]);
  useEffect(() => {
    if (fade.paused) run.current?.pause();
    else run.current?.play();
  }, [fade.paused]);
  return (
    <svg
      aria-hidden="true"
      width={width}
      height={height}
      style={{ position: "absolute", inset: 0, overflow: "visible" }}
    >
      <path d={ring.d} fill="none" stroke={track} strokeWidth={1.5} />
      <path
        ref={line}
        d={ring.d}
        fill="none"
        stroke={color}
        strokeWidth={1.5}
        strokeLinecap="round"
        strokeDasharray={`${ring.length} ${ring.length}`}
      />
    </svg>
  );
}

/** One end of the dock: an icon and how many wait there. On a Simple dock it
 * is a bubble of the dock's own ground; on a Dynamic dock and in the notch it
 * has none of its own. The tail's ring shows when its oldest message goes. */
export function EndPill({
  side,
  rect,
  count,
  mode,
  across,
  light,
  fill,
  outline,
  fade,
  reduceMotion,
  onEnter,
  onLeave,
  onPick,
  zh,
}: {
  side: EndSide;
  rect: HUDRect;
  count: number;
  mode: HUDMode;
  /** The pill lies along a top or bottom edge. */
  across: boolean;
  light: boolean;
  fill: string;
  outline: string;
  fade: EndFade;
  reduceMotion: boolean;
  onEnter: (altKey: boolean) => void;
  onLeave: (event: { relatedTarget?: EventTarget | null }) => void;
  onPick: () => void;
  zh: boolean;
}) {
  const ground = useGround(light);
  const thick = across ? rect.height : rect.width;
  const head = side === "head";
  const Icon = head ? Hand : MessageSquareText;
  const accent = alertText(head ? "warning" : "turn", ground);
  const bare = mode === "logos";
  // On a bare dock the desktop shows behind: a soft shade keeps it legible.
  const shade = bare ? "drop-shadow(0 0 2px rgba(0,0,0,0.55))" : undefined;
  return (
    <motion.div
      className="hud-dock-end"
      data-end={side}
      role="button"
      aria-label={
        head
          ? zh
            ? `${count} 个请求待处理`
            : `${count} requests waiting`
          : zh
            ? `${count} 条消息`
            : `${count} messages`
      }
      initial={reduceMotion ? false : { opacity: 0, scale: 0.55, ...box(rect) }}
      animate={{ opacity: 1, scale: 1, ...box(rect) }}
      exit={reduceMotion ? { opacity: 0 } : { opacity: 0, scale: 0.55 }}
      transition={{
        duration: reduceMotion ? 0 : HUD_METRICS.duration,
        ease: HUD_METRICS.ease,
      }}
      onPointerEnter={(event) => onEnter(event.altKey)}
      onPointerLeave={onLeave}
      onClick={onPick}
      style={{
        position: "absolute",
        ...box(rect),
        zIndex: 3,
        borderRadius: thick / 2,
        background: mode === "simple" ? fill : "transparent",
        boxShadow: mode === "simple" ? `inset 0 0 0 1px ${outline}` : undefined,
        cursor: "default",
        userSelect: "none",
      }}
    >
      {!head && (
        <FadeRing
          width={rect.width}
          height={rect.height}
          fade={fade}
          color={accent}
          track={alertInk(0.14, ground)}
        />
      )}
      {head && mode === "simple" && (
        <span
          aria-hidden="true"
          style={{
            position: "absolute",
            inset: 0.9,
            borderRadius: thick / 2,
            boxShadow: `inset 0 0 0 1.5px ${accent}`,
            opacity: 0.55,
          }}
        />
      )}
      <span
        style={{
          position: "absolute",
          inset: 0,
          display: "flex",
          flexDirection: across ? "row" : "column",
          alignItems: "center",
          justifyContent: "center",
          gap: Math.round(thick * 0.08),
          color: alertInk(0.92, ground),
          filter: shade,
        }}
      >
        <Icon
          size={Math.round(thick * 0.46)}
          strokeWidth={2}
          color={accent}
          aria-hidden="true"
        />
        <motion.span
          key={count}
          initial={reduceMotion ? false : { scale: 1.35 }}
          animate={{ scale: 1 }}
          transition={{ duration: 0.3, ease: HUD_METRICS.ease }}
          style={{
            fontSize: Math.round(thick * 0.42),
            fontWeight: 600,
            lineHeight: 1,
            fontVariantNumeric: "tabular-nums",
          }}
        >
          {count > 99 ? "99+" : count}
        </motion.span>
      </span>
    </motion.div>
  );
}
