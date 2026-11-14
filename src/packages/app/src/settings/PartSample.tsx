import { useFormat } from "@agent-hud/ui/format-context";
import { quotaPercent, quotaPercentLabel } from "@agent-hud/ui/format";
import {
  DollarSign,
  Flame,
  Gauge,
  MessageCircleQuestion,
  Percent,
  Sigma,
} from "lucide-react";
import { HUDArtwork as HUDLogo } from "@agent-hud/hud/HUDArtwork";
import type { HUDConfig } from "@agent-hud/hud/model";
import type { Translate } from "@agent-hud/ui/format";

/** A made-up slice of one panel section, in the HUD's own light or dark
 * look; faded while that section is switched off. */
export function PartSample({
  kind,
  enabled,
  config,
  t,
}: {
  kind: "sessions" | "quota" | "tokens";
  enabled: boolean;
  config: HUDConfig;
  t: Translate;
}) {
  const light =
    config.appearance === "light" ||
    (config.appearance === "system" &&
      !matchMedia("(prefers-color-scheme: dark)").matches);
  const { quotaDisplay } = useFormat();
  const amounts = [4, 8, 5, 11, 16, 9, 6, 13, 20, 14, 10, 7];
  return (
    <div
      aria-hidden="true"
      style={{
        padding: "0 14px",
        height: kind === "sessions" ? 56 : 64,
        display: "flex",
        flexDirection: "column",
        justifyContent: "center",
        gap: kind === "sessions" ? 8 : 10,
        borderRadius: 9,
        background: light ? "#f5f5f7" : "#000",
        color: light ? "#1d1d1f" : "#f5f5f7",
        opacity: enabled ? 1 : 0.25,
        pointerEvents: "none",
        fontSize: 11,
      }}
    >
      {kind === "sessions" ? (
        <>
          <div style={{ display: "flex", justifyContent: "space-between" }}>
            <strong style={{ fontWeight: 600 }}>{t("Sessions")}</strong>
            <span style={{ opacity: 0.55 }}>{t("1 running")}</span>
          </div>
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 6,
              opacity: 0.65,
            }}
          >
            <span style={{ position: "relative", width: 14, height: 14 }}>
              <HUDLogo source="Claude" size={14} light={light} />
              <i
                style={{
                  position: "absolute",
                  width: 6,
                  height: 6,
                  background: "#34c759",
                  borderRadius: "50%",
                  bottom: -2,
                  right: -2,
                  boxShadow: `0 0 0 1.5px ${light ? "#f5f5f7" : "#000"}`,
                }}
              />
            </span>
            <span>{t("Fix sign-in issue")}</span>
            <span style={{ flex: 1 }} />
            <span
              style={{
                display: "flex",
                alignItems: "center",
                gap: 3,
                color: "#e6b94d",
                padding: "3px 6px",
                borderRadius: 20,
                background: "#e6b94d24",
                fontSize: 9,
              }}
            >
              <MessageCircleQuestion size={9} />1
            </span>
            <span>60k tok</span>
          </div>
        </>
      ) : kind === "quota" ? (
        <>
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 6,
              opacity: 0.7,
            }}
          >
            <HUDLogo source="Claude" size={12} light={light} />
            <strong style={{ fontWeight: 600 }}>Claude</strong>
            <span style={{ flex: 1 }} />
            {[Percent, Flame, Sigma, Gauge, DollarSign].map((Icon, index) => (
              <span
                key={index}
                style={{
                  display: "inline-grid",
                  placeItems: "center",
                  width: 20,
                  height: 18,
                  borderRadius: 5,
                  background:
                    index === 0
                      ? light
                        ? "#0000000f"
                        : "#ffffff12"
                      : "transparent",
                  opacity: index === 0 ? 1 : 0.5,
                }}
              >
                <Icon size={10} />
              </span>
            ))}
          </div>
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 10,
              fontSize: 12,
            }}
          >
            <i
              style={{
                width: 7,
                height: 7,
                borderRadius: "50%",
                background: "#34c759",
              }}
            />
            <strong style={{ fontWeight: 600 }}>{t("Session")}</strong>
            <span
              style={{
                flex: 1,
                height: 4,
                background: light ? "#0001" : "#fff1",
                borderRadius: 5,
              }}
            >
              <i
                style={{
                  display: "block",
                  width: `${quotaPercent(28, quotaDisplay)}%`,
                  height: 4,
                  borderRadius: 5,
                  background: "#34c759",
                }}
              />
            </span>
            <strong style={{ color: "#34c759", fontWeight: 600 }}>
              {quotaPercentLabel(28, quotaDisplay)}
            </strong>
            {config.resetCountdown && (
              <span
                style={{
                  width: 78,
                  textAlign: "right",
                  opacity: 0.55,
                  fontSize: 11,
                }}
              >
                {t("2h 14m")}
              </span>
            )}
          </div>
        </>
      ) : (
        <>
          <div style={{ display: "flex", justifyContent: "space-between" }}>
            <strong style={{ fontWeight: 600 }}>{t("Tokens")}</strong>
            <span style={{ opacity: 0.55 }}>{t("1hr · 46M tok")}</span>
          </div>
          <div
            style={{ display: "flex", alignItems: "end", gap: 4, height: 30 }}
          >
            {Array.from({ length: 24 }, (_, i) => (
              <div
                key={i}
                style={{
                  flex: 1,
                  display: "flex",
                  flexDirection: "column",
                  justifyContent: "end",
                  height: `${((amounts[i % 12] + amounts[(i + 4) % 12]) / 38) * 100}%`,
                  minHeight: 2,
                }}
              >
                <i
                  style={{
                    background: "#3aa0ef",
                    height: `${(amounts[(i + 4) % 12] / (amounts[i % 12] + amounts[(i + 4) % 12])) * 100}%`,
                  }}
                />
                <i style={{ background: "#20b9a8", flex: 1 }} />
              </div>
            ))}
          </div>
        </>
      )}
    </div>
  );
}
