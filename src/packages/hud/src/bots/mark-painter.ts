import type { BotActor } from "./bot-actor";
import type { BotForm } from "./bot-form";
import type { ClawdSheet } from "./clawd-sheet";
import type { ClawdActorFrame } from "./clawd-actor";
import { drawsClawdSprites } from "./clawd";
import { stillSign } from "./bot-state";
import { botRenderLook, renderBot, renderLogo } from "./bot-renderer";
import { stillSignImage } from "./bot-signs";

export const clock = () => performance.now() / 1000;

/** The mark's latest props, read on every frame. */
export interface MarkInput {
  size: number;
  enabled: boolean;
  parentAim?: readonly [number, number];
  outline: number;
}

/** BotMark's canvas loop: steps the actor when its frame is due, places the
 * canvas by the bot's pose and paints the still logo, Clawd's sprites or the
 * procedural bot. `wake` paints now, or for a pointer move only once a frame is
 * due; `dispose` stops the loop. */
export function paintMark(
  canvas: HTMLCanvasElement,
  ctx: CanvasRenderingContext2D,
  {
    actor,
    key,
    light,
    artwork,
    form,
    sheet,
    props,
    localAim,
    onReady,
  }: {
    actor: BotActor;
    key: string;
    light: boolean;
    artwork: HTMLCanvasElement;
    form: BotForm | null;
    sheet: ClawdSheet | null;
    props: { readonly current: MarkInput };
    localAim: { readonly current: readonly [number, number] };
    onReady: () => void;
  },
) {
  let disposed = false,
    timer: ReturnType<typeof setTimeout> | undefined;
  const media = matchMedia("(prefers-reduced-motion: reduce)");
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
            const mark = stillSignImage(sign, null, input.size, scale, light);
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
          else drawLogo(seen.sign ?? null);
        };
        if (frame.back) draw(frame.back, frame.backOpacity);
        draw(frame.front, frame.frontOpacity);
        ctx.globalAlpha = 1;
      } else if (form) {
        const next = `${botRenderLook(actor.motion)}:${side}:${hair}`;
        if (next !== imageKey) {
          image = renderBot(form, actor.motion, side, hair, image ?? undefined);
          imageKey = next;
        }
        if (image) ctx.drawImage(image, pad, pad);
      }
      paintedKey = nextPaint;
    }
    if (!announced) {
      announced = true;
      onReady();
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
  const wake = (aimOnly = false) => {
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
  step();
  return {
    wake,
    dispose() {
      disposed = true;
      clearTimeout(timer);
      media.removeEventListener("change", resourceChanged);
      window.removeEventListener("resize", resume);
      document.removeEventListener("visibilitychange", resume);
    },
  };
}
