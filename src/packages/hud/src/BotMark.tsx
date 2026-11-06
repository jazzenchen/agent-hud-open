import { useEffect, useMemo, useRef, useState } from "react";
import { botKey, loadArtwork } from "./bots/artwork";
import type { MarkState } from "./bots/clawd";
import { loadClawd } from "./bots/clawd-sheet";
import { BotActor } from "./bots/bot-actor";
import { botPhaseState, type BotState, type BotCue } from "./bots/bot-state";
import { makeForm } from "./bots/bot-form";
import { clock, paintMark } from "./bots/mark-painter";
import { HUDArtwork } from "./HUDArtwork";
import { releaseCanvas } from "./release-canvas";

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
      painter: ReturnType<typeof paintMark> | undefined;
    void Promise.all([
      loadArtwork(key, light),
      key === "claude" ? loadClawd().catch(() => null) : null,
    ]).then(([artwork, sheet]) => {
      if (disposed || !artwork) return;
      const form = makeForm(key, artwork, light);
      if (form) actor.fit(form);
      if (sheet) actor.clawd.useSheet(sheet);
      painter = paintMark(canvas, ctx, {
        actor,
        key,
        light,
        artwork,
        form,
        sheet,
        props,
        localAim,
        onReady: () => setReady(true),
      });
      wake.current = painter.wake;
    });
    return () => {
      disposed = true;
      painter?.dispose();
      wake.current = null;
      releaseCanvas(canvas);
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
