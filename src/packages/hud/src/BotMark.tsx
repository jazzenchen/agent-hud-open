import { useEffect, useMemo, useRef, useState } from "react";
import { botKey, loadArtwork } from "./bots/artwork";
import { drawsClawdSprites, loadClawd, type MarkState } from "./bots/clawd";
import { BotActor } from "./bots/bot-actor";
import {
  botPhaseState,
  stillSign,
  type BotState,
  type BotCue,
} from "./bots/bot-state";
import { makeForm } from "./bots/bot-form";
import { botRenderLook, renderBot, renderLogo } from "./bots/bot-renderer";
import { stillSignImage } from "./bots/bot-signs";
import type { ClawdActorFrame } from "./bots/clawd-actor";
import { HUDArtwork } from "./HUDArtwork";

export interface BotMarkProps {
  source: string;
  size: number;
  state: BotState | MarkState;
  cue?: BotCue | null;
  enabled?: boolean;
  light?: boolean;
  hovered?: boolean;
  aim?: readonly [number, number];
  staysBot?: boolean;
  arrivesAfter?: number;
  awake?: boolean;
  outline?: number;
}
const clock = () => performance.now() / 1000;
const stateOf = (state: BotState | MarkState): BotState =>
  typeof state === "string" ? botPhaseState(state) : state;
/** Complete source BotActor/ClawdActor, with drawing separate from state delivery. */
export function BotMark({
  source,
  size,
  state,
  cue,
  enabled = true,
  light = false,
  hovered,
  aim: parentAim,
  staysBot = false,
  arrivesAfter = 0,
  awake = false,
  outline = 0,
}: BotMarkProps) {
  const key = botKey(source),
    canvasRef = useRef<HTMLCanvasElement>(null);
  const wake = useRef<((aimOnly?: boolean) => void) | null>(null),
    localHover = useRef(false),
    localAim = useRef<readonly [number, number]>([0, 0]);
  const props = useRef({
    size,
    state,
    cue,
    enabled,
    hovered,
    parentAim,
    outline,
  });
  props.current = { size, state, cue, enabled, hovered, parentAim, outline };
  const actor = useMemo(
    () =>
      new BotActor(
        { ...stateOf(state), hovered: hovered ?? stateOf(state).hovered },
        clock(),
        { staysBot, arrivesAfter, awake },
      ),
    [key, staysBot, arrivesAfter],
  );
  const [ready, setReady] = useState(false);
  const send = () => {
    const value = props.current,
      base = stateOf(value.state);
    const now = clock();
    const changed = actor.set(
      {
        ...base,
        hovered: value.hovered ?? (base.hovered || localHover.current),
      },
      now,
    );
    const played = actor.playCue(value.cue, now);
    if (changed || played) wake.current?.();
  };
  const semanticState = stateOf(state);
  useEffect(send, [
    actor,
    semanticState.look,
    semanticState.subagents,
    semanticState.badge,
    semanticState.holds,
    semanticState.card,
    semanticState.facing,
    semanticState.hovered,
    cue?.event,
    cue?.at,
    hovered,
  ]);
  useEffect(() => {
    actor.invalidate();
    wake.current?.();
  }, [actor, size, enabled, outline]);
  useEffect(() => {
    wake.current?.(true);
  }, [parentAim?.[0], parentAim?.[1]]);
  useEffect(() => {
    setReady(false);
    const canvas = canvasRef.current,
      ctx = canvas?.getContext("2d");
    if (!canvas || !ctx) return;
    let disposed = false,
      timer: ReturnType<typeof setTimeout> | undefined;
    const media = matchMedia("(prefers-reduced-motion: reduce)");
    let clean = () => {};
    void Promise.all([
      loadArtwork(key, light),
      key === "claude" ? loadClawd().catch(() => null) : null,
    ])
      .then(([artwork, sheet]) => {
        if (disposed || !artwork) return;
        const form = makeForm(key, artwork, light);
        if (form) actor.fit(form);
        if (sheet) actor.clawd.useSheet(sheet);
        let imageKey = "",
          image: HTMLCanvasElement | null = null;
        let paintedKey = "",
          layoutKey = "",
          placed = "",
          opacity = -1,
          announced = false,
          scheduledDue = Infinity;
        const logos = new Map<string, HTMLCanvasElement>();
        const cancelTimer = () => {
          clearTimeout(timer);
          timer = undefined;
          scheduledDue = Infinity;
        };
        const arm = (due: number) => {
          if (!Number.isFinite(due)) {
            cancelTimer();
            return;
          }
          if (timer !== undefined && due === scheduledDue) return;
          cancelTimer();
          scheduledDue = due;
          timer = setTimeout(
            () => {
              timer = undefined;
              scheduledDue = Infinity;
              step();
            },
            Math.max(1, (due - clock()) * 1000),
          );
        };
        const step = () => {
          if (disposed) return;
          if (document.hidden) {
            cancelTimer();
            return;
          }
          const now = clock(),
            input = props.current,
            aim = input.parentAim ?? localAim.current;
          const scale = devicePixelRatio || 1,
            side = input.size * scale,
            hair = input.outline * scale,
            // Room round Claude's frame for Clawd's props (desk, laptop, light
            // bulb, spark) at the grain this app rounds to (clawdGrain).
            pad = key === "claude" ? Math.ceil(side * 0.3) : 0,
            extent = Math.ceil(side * 1.6) + hair * 2 + pad * 2;
          const layout = `${side}:${scale}:${hair}:${extent}`;
          const still = media.matches || !input.enabled || !form;
          const sprites = Boolean(
            key === "claude" && sheet && drawsClawdSprites(input.size, scale),
          );
          if (!still) actor.selectRenderer(sprites ? "clawd" : "procedural");
          if (layout !== layoutKey) {
            layoutKey = layout;
            actor.invalidate();
            if (canvas.width !== extent || canvas.height !== extent)
              canvas.width = canvas.height = extent;
            const cssExtent = `${extent / scale}px`,
              cssOffset = `${-(extent / scale - input.size) / 2}px`;
            canvas.style.width = canvas.style.height = cssExtent;
            canvas.style.left = canvas.style.top = cssOffset;
            paintedKey = "";
            imageKey = "";
            logos.clear();
          }
          // Pointer wakes only update the aim retained in props. A not-yet-due actor
          // keeps its existing timer and never advances either renderer early.
          if (!still && now < actor.due - 0.004) {
            arm(actor.due);
            return;
          }
          const result = still
            ? null
            : actor.step(now, aim, sprites ? "clawd" : "procedural");
          const frame = result?.frame ?? null;
          const look = still
            ? `still:${stillSign(actor.state)}`
            : sprites && frame
              ? `clawd:${JSON.stringify([frame.front, frame.back, frame.frontOpacity, frame.backOpacity, frame.opacity])}`
              : `body:${botRenderLook(actor.motion)}`;
          const nextPaint = `${layout}:${look}`;
          const pose = !still && !sprites ? actor.motion.pose : null;
          const transform = pose
            ? `translate(${pose.x * input.size}px, ${(pose.y + pose.footing) * input.size}px) rotate(${pose.rotation}deg) scale(${pose.scaleX}, ${pose.scaleY}) translateY(${-pose.footing * input.size}px)`
            : "none";
          const alpha = !still && !sprites ? actor.motion.opacity : 1;
          if (transform !== placed) {
            canvas.style.transform = transform;
            placed = transform;
          }
          if (alpha !== opacity) {
            canvas.style.opacity = String(alpha);
            opacity = alpha;
          }
          if (nextPaint !== paintedKey) {
            ctx.setTransform(1, 0, 0, 1, 0, 0);
            ctx.clearRect(0, 0, extent, extent);
            ctx.globalAlpha = 1;
            const drawLogo = (sign: ReturnType<typeof stillSign>) => {
              if (form) {
                const key = sign ?? "logo";
                let logo = logos.get(key);
                if (!logo) {
                  logo = renderLogo(form, sign, side, hair);
                  logos.set(key, logo);
                }
                ctx.drawImage(logo, pad, pad);
              } else {
                ctx.drawImage(
                  artwork,
                  pad + hair + side * 0.3,
                  pad + hair + side * 0.3,
                  side,
                  side,
                );
                if (sign) {
                  const mark = stillSignImage(
                    sign,
                    null,
                    input.size,
                    scale,
                    light,
                  );
                  ctx.drawImage(
                    mark.image,
                    pad + hair + side * 0.3 + mark.frame.x * scale,
                    pad + hair + side * 0.3 + mark.frame.y * scale,
                  );
                }
              }
            };
            if (still) drawLogo(stillSign(actor.state));
            else if (sprites && frame) {
              const draw = (seen: ClawdActorFrame["front"], alpha: number) => {
                if (alpha <= 0) return;
                ctx.globalAlpha = alpha * frame.opacity;
                const baked =
                  seen.picture &&
                  actor.clawd.baker?.image(
                    seen.picture,
                    input.size,
                    scale,
                    extent,
                    light,
                    hair,
                  );
                if (baked) ctx.drawImage(baked, 0, 0);
                else
                  drawLogo(
                    seen.sign
                      ? (
                          {
                            "!": "exclamation",
                            "?": "question",
                            "×": "cross",
                            "⧗": "hourglass",
                            z: "z",
                          } as const
                        )[seen.sign]
                      : null,
                  );
              };
              if (frame.back) draw(frame.back, frame.backOpacity);
              draw(frame.front, frame.frontOpacity);
              ctx.globalAlpha = 1;
            } else if (form) {
              const next = `${botRenderLook(actor.motion)}:${side}:${hair}`;
              if (next !== imageKey) {
                image = renderBot(
                  form,
                  actor.motion,
                  side,
                  hair,
                  image ?? undefined,
                );
                imageKey = next;
              }
              if (image) ctx.drawImage(image, pad, pad);
            }
            paintedKey = nextPaint;
          }
          if (!announced) {
            announced = true;
            setReady(true);
          }
          if (still) cancelTimer();
          else arm(result!.due);
        };
        const resume = () => {
          step();
        };
        const resourceChanged = () => {
          actor.invalidate();
          step();
        };
        wake.current = (aimOnly = false) => {
          // Aim is retained for the existing due frame. A still mark, or an actor
          // already waiting on its timer, needs no additional pointer wake.
          if (
            aimOnly &&
            (media.matches ||
              !props.current.enabled ||
              !form ||
              clock() < actor.due - 0.004)
          )
            return;
          step();
        };
        media.addEventListener("change", resourceChanged);
        window.addEventListener("resize", resume);
        document.addEventListener("visibilitychange", resume);
        clean = () => {
          media.removeEventListener("change", resourceChanged);
          window.removeEventListener("resize", resume);
          document.removeEventListener("visibilitychange", resume);
        };
        step();
      })
      .catch((error) => {
        console.error("Bot artwork unavailable", error);
      });
    return () => {
      disposed = true;
      clearTimeout(timer);
      clean();
      wake.current = null;
    };
  }, [actor, key, light]);
  return (
    <span
      role="img"
      aria-label={`${source}: ${stateOf(state).look}`}
      data-bot-source={key}
      data-bot-state={stateOf(state).look}
      style={{
        display: "inline-block",
        flexShrink: 0,
        width: size,
        height: size,
        position: "relative",
        overflow: "visible",
      }}
      onPointerEnter={() => {
        localHover.current = true;
        if (props.current.hovered === undefined) send();
      }}
      onPointerMove={(event) => {
        if (props.current.parentAim) return;
        const rect = event.currentTarget.getBoundingClientRect();
        localAim.current = [
          Math.max(
            -1,
            Math.min(
              1,
              (event.clientX - rect.x - rect.width / 2) /
                Math.max(1, rect.width),
            ),
          ),
          Math.max(
            -1,
            Math.min(
              1,
              (event.clientY - rect.y - rect.height / 2) /
                Math.max(1, rect.height),
            ),
          ),
        ];
        wake.current?.(true);
      }}
      onPointerLeave={() => {
        localHover.current = false;
        if (props.current.hovered === undefined) send();
      }}
    >
      {!ready && <HUDArtwork source={source} size={size} light={light} />}
      <canvas
        ref={canvasRef}
        aria-hidden="true"
        style={{
          position: "absolute",
          // Inherited once drawn, never forced visible: the HUD's hidden
          // measuring copy of a card must not show its marks.
          visibility: ready ? undefined : "hidden",
          pointerEvents: "none",
        }}
      />
    </span>
  );
}
