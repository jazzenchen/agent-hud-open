import { useEffect, useLayoutEffect, useRef } from "react";
import { motion } from "motion/react";
import { Hand, MessageSquareText } from "lucide-react";
import type { HUDRect } from "../geometry";
import { HUD_METRICS, type HUDMode } from "../model";
import { useGround } from "../ground";
import { alertInk, alertText } from "../tokens";
import { endBadge, ringPath, type EndSide } from "./end-geometry";
import type { EndFade } from "./useParkedEnds";

const box = (rect: HUDRect) => ({
  left: rect.x,
  top: rect.y,
  width: rect.width,
  height: rect.height,
});

/** The tail's countdown, drawn as the bubble's own hairline lit up: full
 * when a round starts, gone when its oldest message goes. It stops with the
 * round's clock. `track` is the unlit hairline where the bubble has none. */
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
  track?: string;
}) {
  const ring = ringPath(width, height, 0.5);
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
      // In small steps, about 20 a second: a hairline this slow needs no
      // more, and each step repaints it.
      {
        duration: fade.ms,
        easing: `steps(${Math.round(fade.ms / 50)}, end)`,
        fill: "forwards",
      },
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
      {track && <path d={ring.d} fill="none" stroke={track} strokeWidth={1} />}
      <path
        ref={line}
        d={ring.d}
        fill="none"
        stroke={color}
        strokeWidth={1}
        strokeDasharray={`${ring.length} ${ring.length}`}
      />
    </svg>
  );
}

/** One end of the dock: a circle with its icon, and how many wait there in a
 * badge on its edge. On a Simple dock the circle has the dock's own ground
 * and hairline; in the notch it has none of its own.
 * The tail's hairline runs down to when its oldest message goes. */
export function EndPill({
  side,
  rect,
  count,
  mode,
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
  const thick = Math.min(rect.width, rect.height);
  const badge = endBadge(thick, count);
  const head = side === "head";
  const Icon = head ? Hand : MessageSquareText;
  const accent = alertText(head ? "warning" : "turn", ground);
  const grounded = mode === "simple";
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
      // Only opacity and scale move: they run on the compositor, so the pop
      // keeps its pace while the page is busy.
      initial={reduceMotion ? false : { opacity: 0, scale: 0.55 }}
      animate={{ opacity: 1, scale: 1 }}
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
        borderRadius: "50%",
        background: grounded ? fill : "transparent",
        boxShadow: grounded ? `inset 0 0 0 1px ${outline}` : undefined,
        cursor: "default",
        userSelect: "none",
      }}
    >
      {!head && fade.ms > 0 && (
        <FadeRing
          width={rect.width}
          height={rect.height}
          fade={fade}
          color={alertInk(0.55, ground)}
          track={grounded ? undefined : alertInk(0.16, ground)}
        />
      )}
      <span
        style={{
          position: "absolute",
          inset: 0,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        <Icon
          size={badge.icon}
          strokeWidth={2}
          color={accent}
          aria-hidden="true"
        />
      </span>
      <motion.span
        key={count}
        aria-hidden="true"
        initial={reduceMotion ? false : { scale: 1.35 }}
        animate={{ scale: 1 }}
        transition={{ duration: 0.3, ease: HUD_METRICS.ease }}
        style={{
          position: "absolute",
          left: badge.x,
          top: badge.y,
          height: badge.size,
          minWidth: badge.size,
          translate: "-50% -50%",
          padding: "0 3px",
          boxSizing: "border-box",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          borderRadius: badge.size / 2,
          // The badge wears the dock's own ground and hairline.
          background: fill,
          boxShadow: `inset 0 0 0 1px ${outline}`,
          color: alertInk(0.92, ground),
          fontSize: badge.font,
          fontWeight: 400,
          lineHeight: 1,
          fontVariantNumeric: "tabular-nums",
        }}
      >
        {badge.label}
      </motion.span>
    </motion.div>
  );
}
