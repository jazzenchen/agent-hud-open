import type { CSSProperties } from "react";
import type { IslandEvent } from "@agent-hud/protocol";
import { agentVendorName } from "../model";
import { alertFill, alertInk, alertText, onAlertFill } from "../tokens";
import { useGround } from "../ground";
import { cardColumn } from "../card-column";
import { usePanelServices } from "../PanelContext";
import { Heading } from "./AlertCardParts";
import { useT } from "@agent-hud/ui/format-context";

/** An agent that skips Agent HUD's hooks until the user allows them in the
 * agent itself (Codex keeps a changed hook "modified"): what that costs,
 * where to allow them (`step`, in the agent's own words), and a way to it.
 * Told on the island only, at most once a day while they wait. */
export function HooksReviewAlertView({
  event,
  dismiss,
  light = false,
  botLogos = true,
}: {
  event: IslandEvent;
  dismiss: () => void;
  light?: boolean;
  botLogos?: boolean;
}) {
  const t = useT();
  const ground = useGround(light);
  const services = usePanelServices();
  const ink = (alpha: number) => alertInk(alpha, ground);
  const name = event.title ?? agentVendorName(event.source);
  const button: CSSProperties = {
    height: 30,
    flex: 1,
    borderRadius: 7,
    fontSize: 12,
    fontWeight: 600,
    cursor: "pointer",
  };
  const open = services?.openClient;
  return (
    <article style={{ ...cardColumn(14), fontSize: 12, color: "inherit" }}>
      <Heading
        event={event}
        name={name}
        under={null}
        news={t("allow its hooks")}
        accentFill={alertFill("warning", ground)}
        accentText={alertText("warning", ground)}
        light={light}
        botLogos={botLogos}
      />
      {event.message && (
        <div style={{ fontSize: 12, lineHeight: 1.45, color: ink(0.78) }}>
          {event.message}
        </div>
      )}
      {event.step && (
        <div style={{ fontSize: 10, lineHeight: 1.45, color: ink(0.5) }}>
          {event.step}
        </div>
      )}
      <div style={{ display: "flex", gap: 8 }}>
        {open && (
          <button
            type="button"
            onClick={dismiss}
            style={{
              ...button,
              border: `1px solid ${ink(0.18)}`,
              background: "transparent",
              color: ink(0.72),
            }}
          >
            {t("Later")}
          </button>
        )}
        <button
          type="button"
          onClick={() => {
            open?.(event.source);
            dismiss();
          }}
          style={{
            ...button,
            border: 0,
            background: alertFill("warning", ground),
            color: onAlertFill({ light }),
            opacity: 0.9,
          }}
        >
          {open ? t("Open {{name}}", { name }) : t("OK [acknowledge]")}
        </button>
      </div>
    </article>
  );
}
