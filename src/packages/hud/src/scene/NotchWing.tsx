import type { RefObject } from "react";
import { ClipboardCopy } from "lucide-react";
import { BotMark } from "../BotMark";
import { botState, type BotCue } from "../bots/bot-state";
import { islandPath } from "../geometry";
import { PERSON } from "./flight-queue";
import type { NotchStage } from "./flight-route";

// At work, so each is drawn as its robot from the first frame (a calm mark
// would first show its logo and turn into the robot after a second).
const facing = (side: "left" | "right") =>
  botState({ look: "plain", facing: side === "left" ? "right" : "left" });

/** Around the notch, where agents have no marks: a small wing growing out of
 * the notch's side, with the agent's robot peeking out of it, or for the
 * person the clipboard end's icon. */
export function NotchWing({
  side,
  stage,
  source,
  sending,
  puzzled,
  botLogos,
  wingRef,
  botRef,
}: {
  side: "left" | "right";
  stage: NotchStage;
  source: string;
  /** This side sends the flight: its robot is left puzzled by a fall. */
  sending: boolean;
  puzzled: BotCue | null;
  botLogos: boolean;
  wingRef: RefObject<SVGSVGElement | null>;
  botRef: RefObject<HTMLDivElement | null>;
}) {
  const wing = stage.wings[side];
  return (
    <div>
      <svg
        ref={wingRef}
        width={wing.width}
        height={stage.height}
        style={{
          position: "absolute",
          left: wing.x,
          top: stage.top,
          overflow: "visible",
          transformOrigin: side === "left" ? "right center" : "left center",
          scale: "0 1",
        }}
      >
        <path
          d={islandPath(wing.width, stage.height, stage.flare, stage.radius)}
          fill="#000"
        />
      </svg>
      <div
        ref={botRef}
        style={{
          position: "absolute",
          left:
            (side === "left" ? stage.left.x : stage.right.x) - stage.bot / 2,
          top: stage.top + (stage.height - stage.bot) / 2,
          width: stage.bot,
          height: stage.bot,
          opacity: 0,
        }}
      >
        {source === PERSON ? (
          <ClipboardCopy
            size={Math.round(stage.bot * 0.62)}
            strokeWidth={2}
            color="rgba(255,255,255,0.86)"
            style={{ display: "block", margin: "auto", height: "100%" }}
          />
        ) : (
          <BotMark
            source={source}
            size={stage.bot}
            state={facing(side)}
            cue={sending ? puzzled : null}
            enabled={botLogos}
            light={false}
            awake
          />
        )}
      </div>
    </div>
  );
}
