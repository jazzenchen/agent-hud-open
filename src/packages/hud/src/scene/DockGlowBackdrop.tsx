import { horizontal, type HUDRect } from "../geometry";
import HUDGlow, { type GlowAlert } from "../HUDGlow";
import type { HUDConfig, HUDEdge, HUDVendor } from "../model";

/** A Dynamic dock's glow, drawn behind its marks. */
export function DockGlowBackdrop({
  config,
  edge,
  strip,
  vendors,
  glowColors,
  glowAlert,
  reduceMotion,
}: {
  config: HUDConfig;
  edge: HUDEdge;
  strip: HUDRect;
  vendors: HUDVendor[];
  glowColors: string[];
  glowAlert?: GlowAlert | null;
  reduceMotion: boolean;
}) {
  // ScreenHUD.surface: a Dynamic backdrop is a flat lip, clipped to the marks' column.
  const backdropOverhang = config.glowRange + config.glowBlur * 3 + 200;
  const backdropReach =
    config.glowStyle === "blur"
      ? config.glowRange + config.glowBlur * 3
      : (config.glowGridCore + config.glowGridFade + 1) * config.glowGridPitch;
  // The glow is drawn for a top dock and turned to the edge so its agent
  // colours stay in mark order: left to right on a bottom dock (a vertical
  // flip, not a half turn), top to bottom on the side docks (Open
  // GlowWindowController's canvas orientation).
  const backdropTransform =
    edge === "bottom"
      ? `matrix(1,0,0,-1,0,${strip.height})`
      : edge === "left"
        ? "matrix(0,1,1,0,0,0)"
        : edge === "right"
          ? `matrix(0,1,-1,0,${strip.width},0)`
          : undefined;

  return (
    <div
      aria-hidden="true"
      style={{
        position: "absolute",
        inset: 0,
        pointerEvents: "none",
        clipPath: horizontal(edge)
          ? `inset(-${backdropReach}px 0 -${backdropReach}px 0)`
          : `inset(0 -${backdropReach}px 0 -${backdropReach}px)`,
      }}
    >
      <div
        style={{
          position: "absolute",
          left: 0,
          top: 0,
          transformOrigin: "0 0",
          transform: backdropTransform,
        }}
      >
        <div
          style={{
            position: "absolute",
            left: -backdropOverhang,
            top: -2,
          }}
        >
          <HUDGlow
            outwardOnly={config.glowOutwardOnly}
            style={config.glowStyle}
            effect={config.glowEffect}
            width={
              (horizontal(edge) ? strip.width : strip.height) +
              backdropOverhang * 2
            }
            height={2}
            radius={0}
            attached
            colors={glowColors}
            alert={glowAlert}
            active={vendors.some((v) => v.working > 0)}
            period={
              vendors.some((v) => v.working > 0)
                ? config.breathSeconds
                : config.idleBreathSeconds
            }
            amplitude={config.breathAmplitude}
            brightness={config.glowBrightness}
            range={config.glowRange}
            blur={config.glowBlur}
            pitch={config.glowGridPitch}
            core={config.glowGridCore}
            fade={config.glowGridFade}
            density={config.glowGridDensity}
            reduceMotion={reduceMotion}
          />
        </div>
      </div>
    </div>
  );
}
