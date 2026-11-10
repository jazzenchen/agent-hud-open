import { useEffect, useRef, useState } from "react";
import settingsWallpaper from "@agent-hud/ui/assets/settings-wallpaper.png";
import type { Snapshot } from "@agent-hud/protocol";
import { BotMark } from "@agent-hud/hud/BotMark";
import { HUDGlow } from "@agent-hud/hud/HUDGlow";
import {
  defaultHUDConfig,
  meterLabel,
  statusColor,
  configuredVendors,
  compactLabel,
  type HUDConfig,
} from "@agent-hud/hud/model";
import { islandPath, stripMetrics } from "@agent-hud/hud/geometry";
import {
  DOCK_SCALE,
  dockFontWeight,
  dockScale,
} from "@agent-hud/hud/dock-scale";
import { isRingStyle } from "@agent-hud/hud/usage-meter/arcs";
import {
  MarkBars,
  MarkRing,
  markItems,
} from "@agent-hud/hud/usage-meter/mark-usage";

const originalWallpaper = settingsWallpaper;

/** The resting HUD on a strip of wallpaper, drawn from the screen's settings
 * and the agents in the snapshot. Every mode sits at the top edge of the same
 * box, shrunk to fit when it is wider than the preview; it never opens a
 * panel, and draws nothing while the Settings window is inactive. */
export function SmallHUDPreview({
  snapshot,
  config,
  active,
}: {
  snapshot: Snapshot;
  config: HUDConfig;
  active: boolean;
}) {
  const sample = { ...defaultHUDConfig, ...config, edge: "top" as const };
  const vendors = configuredVendors(snapshot, sample);
  const compact = sample.mode === "simple" && sample.compactDock;
  const hasActiveVendor = vendors.some(
    (vendor) => vendor.working > 0 || vendor.waiting > 0,
  );
  const metrics = stripMetrics(sample, vendors, snapshot.settings.quotaDisplay);
  const simple = sample.mode === "simple";
  const light =
    sample.appearance === "light" ||
    (sample.appearance === "system" &&
      !matchMedia("(prefers-color-scheme: dark)").matches);
  const alpha = 1 - sample.solidTransparency;
  const surface = light ? `rgba(245,245,247,${alpha})` : `rgba(0,0,0,${alpha})`;
  const stage = useRef<HTMLDivElement>(null);
  const [stageWidth, setStageWidth] = useState(0);
  useEffect(() => {
    const element = stage.current;
    if (!element) return;
    const observer = new ResizeObserver(([entry]) =>
      setStageWidth(entry.contentRect.width),
    );
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  // A floating strip keeps a little wallpaper round it; an attached one
  // reaches the edge like the real notch.
  const margin = simple ? 24 : 0;
  const scale =
    stageWidth > 0
      ? Math.min(1, (stageWidth - margin * 2) / Math.max(metrics.width, 1))
      : 1;
  return (
    <div style={{ padding: "12px 14px" }}>
      <div
        ref={stage}
        aria-hidden="true"
        style={{
          position: "relative",
          height: 112,
          borderRadius: 8,
          overflow: "hidden",
          pointerEvents: "none",
          backgroundImage: `url("${originalWallpaper}")`,
          backgroundSize: "cover",
          backgroundPosition: "center",
        }}
      >
        {active && (
          <div
            style={{
              position: "absolute",
              left: "50%",
              top: simple ? 10 : 0,
              transform: `translateX(-50%) scale(${scale})`,
              transformOrigin: "top center",
              width: metrics.width,
              height: metrics.height,
            }}
          >
            {!simple && (
              <HUDGlow
                outwardOnly={sample.glowOutwardOnly}
                width={metrics.width}
                height={metrics.height}
                radius={metrics.radius}
                attached={sample.mode === "notch"}
                style={sample.glowStyle}
                effect={sample.glowEffect}
                colors={vendors
                  .filter((vendor) => vendor.used !== null)
                  .map((vendor) => statusColor(vendor.level))}
                active={hasActiveVendor}
                period={sample.breathSeconds}
                amplitude={sample.breathAmplitude}
                brightness={sample.glowBrightness}
                range={sample.glowRange}
                blur={sample.glowBlur}
                pitch={sample.glowGridPitch}
                core={sample.glowGridCore}
                fade={sample.glowGridFade}
                density={sample.glowGridDensity}
                reduceMotion={!sample.animations}
              />
            )}
            <div
              style={{
                position: "absolute",
                inset: 0,
                display: "flex",
                alignItems: "center",
                gap: metrics.gap,
                padding: simple
                  ? `${metrics.padding.top}px ${metrics.padding.right}px ${metrics.padding.bottom}px ${metrics.padding.left}px`
                  : 0,
                borderRadius: simple ? metrics.radius : "0 0 10px 10px",
                background: simple ? surface : "transparent",
                color: light ? "#161618" : "#f2f2f5",
                boxShadow: simple
                  ? `0 1px 3px rgba(0,0,0,${0.133 * alpha})`
                  : undefined,
              }}
            >
              {sample.mode === "notch" && (
                <svg
                  width={metrics.width + 16}
                  height={metrics.height}
                  style={{
                    position: "absolute",
                    left: -8,
                    top: 0,
                    overflow: "visible",
                  }}
                >
                  <path
                    d={islandPath(
                      metrics.width + 16,
                      metrics.height,
                      8,
                      metrics.radius,
                    )}
                    fill={sample.hasNotch ? "#000" : surface}
                  />
                </svg>
              )}
              {sample.mode !== "notch" &&
                vendors.map((vendor, index) => (
                  <div
                    key={vendor.key}
                    style={{
                      display: "flex",
                      alignItems: "center",
                      gap: metrics.labelGap,
                      flexShrink: 0,
                    }}
                  >
                    <span
                      style={{
                        position: "relative",
                        width: metrics.mark,
                        height: metrics.mark,
                        display: "inline-grid",
                        placeItems: "center",
                      }}
                    >
                      {simple &&
                        !compact &&
                        isRingStyle(sample.simpleStyle) && (
                          <MarkRing
                            style={sample.simpleStyle}
                            items={markItems(
                              vendor,
                              snapshot.settings.quotaDisplay,
                              sample.panelLayout,
                            )}
                            size={metrics.mark}
                            ringWidth={
                              sample.ringWidth *
                              DOCK_SCALE *
                              dockScale(sample.dockScale)
                            }
                            light={light}
                            reduceMotion={!sample.animations}
                          />
                        )}
                      <BotMark
                        source={vendor.name}
                        size={metrics.logo}
                        light={light}
                        enabled={sample.botLogos}
                        state={
                          vendor.waiting > 0
                            ? "asking"
                            : vendor.working > 0 ||
                                (!hasActiveVendor && index === 0)
                              ? "working"
                              : "idle"
                        }
                      />
                    </span>
                    {simple && metrics.bar > 0 && (
                      <MarkBars
                        items={markItems(
                          vendor,
                          snapshot.settings.quotaDisplay,
                          sample.panelLayout,
                        )}
                        length={metrics.bar}
                        height={metrics.mark}
                        size={
                          Math.max(20, Math.min(60, sample.simpleSize)) *
                          DOCK_SCALE *
                          dockScale(sample.dockScale)
                        }
                        ringWidth={
                          sample.ringWidth *
                          DOCK_SCALE *
                          dockScale(sample.dockScale)
                        }
                        light={light}
                        reduceMotion={!sample.animations}
                      />
                    )}
                    {simple &&
                      (compact ||
                        sample.showsNumbers ||
                        sample.simpleStyle === "numbers") && (
                        <span
                          style={{
                            fontSize: metrics.font,
                            fontWeight: dockFontWeight(sample.dockScale),
                            color: statusColor(vendor.level),
                            fontVariantNumeric: "tabular-nums",
                          }}
                        >
                          {compact
                            ? compactLabel(
                                vendor,
                                meterLabel(
                                  vendor,
                                  snapshot.settings.quotaDisplay,
                                ),
                              )
                            : meterLabel(
                                vendor,
                                snapshot.settings.quotaDisplay,
                              )}
                        </span>
                      )}
                  </div>
                ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
