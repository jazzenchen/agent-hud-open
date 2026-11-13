// What the guide's window beside the HUD holds: the tour's steps there, a
// pointer back to a tour left open in Settings, or a release's news with the
// Star last. Drawn in the HUD's own type; the window is GuideDock's.
import { ArrowDown, ArrowLeft, ArrowRight, ArrowUp } from "lucide-react";
import { Button } from "@agent-hud/ui/components/button";
import type { HUDConfig } from "@agent-hud/hud/model";
import { hudStepText, modeStepText, type Place, type Text } from "./copy";
import { DragHint } from "./DragHint";
import { Actions, Body, Header, blue } from "./hud-guide-parts";
import { News } from "./HUDGuideNews";
import type { GuideTarget, GuideView } from "./plan";

export interface HUDGuideCardProps {
  view: GuideView;
  /** The first step's panel has been looked at. */
  looked: boolean;
  place: Place;
  screens: number;
  /** This step's place in the tour, and the tour's length. */
  count: [number, number];
  t: Text;
  /** Animations are on (the app's setting and the system's). */
  moving: boolean;
  next: () => void;
  back: () => void;
  close: () => void;
  resume: () => void;
  show: (target: GuideTarget) => void;
  star: (open: boolean) => void;
  setMode: (mode: HUDConfig["mode"]) => void;
}

const arrows = {
  top: ArrowUp,
  bottom: ArrowDown,
  left: ArrowLeft,
  right: ArrowRight,
};
const nudges = {
  top: "0,-4px",
  bottom: "0,4px",
  left: "-4px,0",
  right: "4px,0",
};

export function HUDGuideCard(props: HUDGuideCardProps) {
  const { view, t, close } = props;
  if (view.kind === "news") return <News {...props} />;
  const label = t(
    `Getting started · ${props.count[0]} / ${props.count[1]}`,
    `上手引导 · ${props.count[0]} / ${props.count[1]}`,
  );
  return (
    <>
      <Header label={label} close={close} t={t} />
      {view.step === "hud" ? (
        <HUDStep {...props} />
      ) : view.step === "mode" ? (
        <ModeStep {...props} />
      ) : (
        <>
          <Body
            title={t("The rest is in Settings", "接下来在设置里")}
            body={t(
              "A few more steps are in Settings. They pick up where you left off.",
              "还有几步在设置里，从刚才停下的地方接着走。",
            )}
          />
          <Actions>
            <Button size="sm" onClick={props.resume}>
              {t("Continue", "继续")}
            </Button>
          </Actions>
        </>
      )}
    </>
  );
}

function HUDStep({ looked, place, t, moving, next }: HUDGuideCardProps) {
  const text = hudStepText(place, looked, t);
  const edge = place.mode === "notch" ? "top" : place.edge;
  const Arrow = arrows[edge];
  const vertical = edge === "top" || edge === "bottom";
  return (
    <>
      <div
        style={{
          display: "flex",
          flexDirection: vertical
            ? edge === "top"
              ? "column-reverse"
              : "column"
            : edge === "left"
              ? "row-reverse"
              : "row",
          alignItems: "center",
          gap: 10,
        }}
      >
        <div style={{ flex: 1, minWidth: 0 }}>
          <Body {...text} />
        </div>
        {!looked && (
          <Arrow
            aria-hidden
            size={22}
            strokeWidth={2.4}
            color={blue}
            style={{
              flexShrink: 0,
              animation: moving
                ? `hud-guide-point-${edge} 1.2s ease-in-out infinite`
                : undefined,
            }}
          />
        )}
      </div>
      <style>
        {`@keyframes hud-guide-point-${edge} { 50% { transform: translate(${nudges[edge]}); } }`}
      </style>
      <Actions>
        <Button size="sm" onClick={next}>
          {t("Next", "下一步")}
        </Button>
      </Actions>
    </>
  );
}

function ModeStep({
  place,
  screens,
  t,
  moving,
  next,
  back,
  setMode,
}: HUDGuideCardProps) {
  const option = (mode: HUDConfig["mode"], label: string) => (
    <Button
      size="sm"
      variant={place.mode === mode ? "default" : "outline"}
      aria-pressed={place.mode === mode}
      onClick={() => setMode(mode)}
      className="flex-1"
    >
      {label}
    </Button>
  );
  return (
    <>
      <Body {...modeStepText(screens, t)} />
      <DragHint moving={moving} />
      <div style={{ display: "flex", gap: 8 }}>
        {option("notch", t("Notch", "刘海"))}
        {option("simple", t("Simple", "简约"))}
      </div>
      <Actions>
        <Button size="sm" variant="ghost" onClick={back} className="mr-auto">
          {t("Back", "上一步")}
        </Button>
        <Button size="sm" onClick={next}>
          {t("Next", "下一步")}
        </Button>
      </Actions>
    </>
  );
}
