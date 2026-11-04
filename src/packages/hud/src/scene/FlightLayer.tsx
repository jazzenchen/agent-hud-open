// A host's flight between two agents' marks, told as a short story: an open
// envelope pops up beside the sender and a letter slides in from its mark; the
// flap closes and the envelope folds into a paper plane; it flies a looping
// route, a dashed trail fading behind it; beside the receiver it unfolds and
// opens, and the letter slides into the receiver's mark, which bounces.
// Around the notch, where agents have no marks, two robots peek out of small
// wings at its sides and play the marks' part. Every part is a Web Animation
// on elements that exist only while a flight is under way: idle, this draws
// nothing and runs nothing.
import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type RefObject,
} from "react";
import type { BotCue } from "../bots/bot-state";
import type { HUDRect } from "../geometry";
import type { HUDEdge, HUDMode } from "../model";
import { craftSize, dockEnds, notchEnds } from "./flight-ends";
import { STORY, story, type Story } from "./flight-plan";
import { playFlight, type FlightPlan } from "./flight-play";
import {
  FLIGHT_FRESHNESS_MS,
  FlightQueue,
  type HUDFlightCommand,
  type Ride,
} from "./flight-queue";
import { notchStage } from "./flight-route";
import {
  Count,
  Envelope,
  Flap,
  Folds,
  Letter,
  Paper,
  Placed,
  Pocket,
  type Tone,
} from "./FlightPaper";
import { NotchWing } from "./NotchWing";

interface LayerProps {
  /** The host's recent flights, oldest first; each is taken once, by its sequence. */
  flights?: readonly HUDFlightCommand[];
  mode: HUDMode;
  edge: HUDEdge;
  /** The notch around the notch; the dock's strip otherwise. */
  strip: HUDRect;
  scene: RefObject<HTMLElement | null>;
  botLogos: boolean;
  reduceMotion: boolean;
  light: boolean;
}

export function FlightLayer({ flights, ...stage }: LayerProps) {
  const queue = useRef(new FlightQueue());
  const seen = useRef(0);
  const [ride, setRide] = useState<Ride | null>(null);
  useEffect(() => {
    let changed = false;
    for (const command of flights ?? []) {
      if (command.sequence <= seen.current) continue;
      seen.current = command.sequence;
      if (performance.now() - command.at < FLIGHT_FRESHNESS_MS)
        changed = queue.current.add(command.flight) || changed;
    }
    if (changed) setRide(queue.current.current);
  }, [flights]);
  if (!ride) return null;
  return (
    <FlightRun
      key={ride.id}
      ride={ride}
      {...stage}
      onDone={() => {
        if (queue.current.finish(ride.id)) setRide(queue.current.current);
      }}
    />
  );
}

function FlightRun({
  ride,
  mode,
  edge,
  strip,
  scene,
  botLogos,
  reduceMotion,
  light,
  onDone,
}: Omit<LayerProps, "flights"> & { ride: Ride; onDone: () => void }) {
  const { flight, count } = ride;
  const notch = mode === "notch";
  const stage = notch ? notchStage(strip) : null;
  // Around the notch the sender stands left or right for its whole chain.
  const leftSource = flight.fromLeft ? flight.from : flight.to;
  const rightSource = flight.fromLeft ? flight.to : flight.from;
  const [size, setSize] = useState(() => craftSize(notch ? 34 : 44));
  const [dashes, setDashes] = useState<Story["dashes"]>([]);
  const craft = useRef<HTMLDivElement>(null);
  const turn = useRef<HTMLDivElement>(null);
  const mirror = useRef<HTMLDivElement>(null);
  const paper = {
    shade: useRef<HTMLDivElement>(null),
    edge: useRef<HTMLDivElement>(null),
    body: useRef<HTMLDivElement>(null),
  };
  const flap = useRef<SVGSVGElement>(null);
  const pocket = useRef<SVGSVGElement>(null);
  const folds = useRef<SVGSVGElement>(null);
  const sheet = useRef<HTMLDivElement>(null);
  const letter = useRef<HTMLDivElement>(null);
  const second = useRef<HTMLDivElement>(null);
  const trail = useRef<HTMLDivElement>(null);
  const wings = {
    left: useRef<SVGSVGElement>(null),
    right: useRef<SVGSVGElement>(null),
  };
  const bots = {
    left: useRef<HTMLDivElement>(null),
    right: useRef<HTMLDivElement>(null),
  };
  const [puzzled, setPuzzled] = useState<BotCue | null>(null);
  const [plan, setPlan] = useState<FlightPlan | null>(null);

  // First: measure where it happens and plan the story (the trail's dashes
  // and the craft's size are drawn from it).
  useLayoutEffect(() => {
    const measured = stage
      ? notchEnds(stage, flight)
      : dockEnds(scene.current!, flight, strip, edge);
    setSize(measured.size);
    const delay = notch ? STORY.enter : 0;
    const planned = reduceMotion
      ? null
      : story({
          route: measured.ends.route,
          from: measured.ends.from,
          to: measured.ends.to,
          falls: flight.falls,
          delay,
        });
    setDashes(planned?.dashes ?? []);
    setPlan({ ends: measured.ends, story: planned });
  }, []);

  // Then: hand every part its keyframes.
  useLayoutEffect(() => {
    if (!plan) return;
    return playFlight(
      {
        craft,
        turn,
        mirror,
        paper,
        flap,
        pocket,
        folds,
        sheet,
        letter,
        second,
        trail,
        wings,
        bots,
      },
      plan,
      { flight, notch, stage, onDone, setPuzzled },
    );
  }, [plan]);

  const ink = light ? "rgba(28,28,36,0.85)" : "rgba(0,0,0,0.6)";
  // The paper's edge and shadow are copies of its outline drawn under it, not
  // CSS filters: the app's compositor drops a filter over animated layers.
  const tone: Tone = {
    ink,
    edge: light ? "rgba(28,28,36,0.8)" : "rgba(0,0,0,0.5)",
    shade: light ? "rgba(0,0,0,0.16)" : "rgba(0,0,0,0.3)",
  };
  // The plane's line, as bold as the reference icon's at this size.
  const line = Math.max(1.3, size.width / 18);
  const dashColor = light ? "rgba(36,36,48,0.75)" : "rgba(255,255,255,0.75)";
  const ends = plan?.ends;
  return (
    <div
      className="hud-flight"
      aria-hidden="true"
      style={{
        position: "absolute",
        inset: 0,
        zIndex: 6,
        pointerEvents: "none",
      }}
    >
      {stage &&
        !reduceMotion &&
        (["left", "right"] as const).map((side) => (
          <NotchWing
            key={side}
            side={side}
            stage={stage}
            source={side === "left" ? leftSource : rightSource}
            sending={(side === "left") === flight.fromLeft}
            puzzled={puzzled}
            botLogos={botLogos}
            wingRef={wings[side]}
            botRef={bots[side]}
          />
        ))}
      <div ref={trail}>
        {dashes.map((dash, index) => (
          <span
            key={index}
            data-dash
            style={{
              position: "absolute",
              left: -3,
              top: -0.9,
              width: 6,
              height: 1.8,
              borderRadius: 1,
              background: dashColor,
              translate: dash.translate,
              rotate: dash.rotate,
              opacity: 0,
            }}
          />
        ))}
      </div>
      <div
        ref={letter}
        style={{
          position: "absolute",
          left: -size.width * 0.31,
          top: -size.height * 0.38,
          width: size.width * 0.62,
          height: size.height * 0.76,
          opacity: 0,
        }}
      >
        <Letter tone={tone} />
      </div>
      {reduceMotion && ends ? (
        <>
          <Placed at={ends.start} refTo={craft} size={size}>
            <Envelope open={false} tone={tone} count={count} />
          </Placed>
          <Placed at={ends.end} refTo={second} size={size}>
            <Envelope open tone={tone} count={1} />
          </Placed>
        </>
      ) : (
        <div ref={craft} style={{ position: "absolute", left: 0, top: 0 }}>
          <div style={{ opacity: 0 }}>
            <div
              ref={turn}
              style={{
                position: "absolute",
                left: -size.width / 2,
                top: -size.height / 2,
                width: size.width,
                height: size.height,
              }}
            >
              <div ref={mirror} style={{ position: "absolute", inset: 0 }}>
                <div style={{ position: "absolute", inset: 0 }}>
                  <div ref={sheet} style={{ position: "absolute", inset: 0 }}>
                    <Paper tone={tone} refs={paper}>
                      <Pocket ink={ink} refTo={pocket} />
                    </Paper>
                  </div>
                  <Flap ink={ink} refTo={flap} open />
                  <Folds
                    tone={tone}
                    line={line}
                    height={size.height}
                    refTo={folds}
                  />
                </div>
              </div>
            </div>
            {count > 1 && <Count value={count} size={size.width} />}
          </div>
        </div>
      )}
    </div>
  );
}
