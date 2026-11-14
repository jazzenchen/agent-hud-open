import type { IslandEvent, Snapshot } from "@agent-hud/protocol";
import { agentVendorName, quotaShortName } from "../model";
import { alertInk } from "../tokens";
import { useGround } from "../ground";
import { cardColumn } from "../card-column";
import { compactAlertLabels } from "./compact-alert-labels";
import { ReminderMark } from "./ReminderMark";
import { useFormat } from "@agent-hud/ui/format-context";

/** A quota or reset-credit event inside the open usage panel: one quiet
 * line, not a nested card (Open QuotaAlertInlineView). Opening it shows the
 * usage. */
export function InlineEventRow({
  event,
  snapshot,
  light = false,
  botLogos = true,
  onOpen,
}: {
  event: IslandEvent;
  snapshot: Snapshot;
  light?: boolean;
  botLogos?: boolean;
  onOpen?: () => void;
}) {
  const ground = useGround(light);
  const { language } = useFormat();
  const { status } = compactAlertLabels(undefined, event, 0, language);
  const quota = snapshot.quotas.find((row) => row.id === event.quotaId);
  const vendor = agentVendorName(event.source);
  const account = quota ? `${vendor} · ${quotaShortName(quota)}` : vendor;
  return (
    <button
      type="button"
      onClick={onOpen}
      aria-label={`${vendor} · ${status}`}
      style={{
        display: "flex",
        alignItems: "center",
        gap: 10,
        width: "100%",
        padding: "8px 0",
        border: 0,
        background: "transparent",
        textAlign: "left",
        cursor: onOpen ? "pointer" : "default",
        color: "inherit",
      }}
    >
      <ReminderMark event={event} size={13} light={light} botLogos={botLogos} />
      <span style={{ ...cardColumn(4), minWidth: 0 }}>
        <span
          style={{
            fontSize: 12,
            fontWeight: 500,
            color: alertInk(0.95, ground),
          }}
        >
          {status}
        </span>
        <span style={{ fontSize: 10, color: alertInk(0.45, ground) }}>
          {account}
        </span>
      </span>
      <span
        aria-hidden="true"
        style={{
          marginLeft: "auto",
          fontSize: 10,
          color: alertInk(0.4, ground),
        }}
      >
        ↗
      </span>
    </button>
  );
}
