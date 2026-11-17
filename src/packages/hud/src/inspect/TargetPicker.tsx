import { ChevronDown } from "lucide-react";
import { useT } from "@agent-hud/ui/format-context";
import { cardColumn } from "../card-column";
import { useGround } from "../ground";
import { alertInk } from "../tokens";
import { AgentTabs, tabClients } from "./AgentTabs";
import { FolderGroups } from "./FolderGroups";
import { Place } from "./Place";
import type { ClipPanel } from "./useClipWindow";

/** The list of places scrolls past this height, so the window stays short. */
const LIST_HEIGHT = 240;

/** Where the window sends: the place chosen, and on a press a tab per
 * client over that client's sessions, grouped by folder. */
export function TargetPicker({
  panel,
  light,
}: {
  panel: ClipPanel;
  light: boolean;
}) {
  const t = useT();
  const ground = useGround(light);
  const { targets, target, picking } = panel;
  const clients = tabClients([
    ...(targets?.sessions ?? []).map((row) => row.client),
    ...(targets?.starts ?? []).map((start) => start.client),
  ]);
  const client =
    panel.tab && clients.includes(panel.tab) ? panel.tab : clients[0];
  const near =
    target?.kind === "session"
      ? targets?.sessions.find((row) => row.id === target.sessionId)
          ?.startFolder
      : target?.folder;
  const chevron = (
    <ChevronDown
      size={13}
      strokeWidth={2}
      color={alertInk(0.45, ground)}
      style={{ flexShrink: 0, rotate: picking ? "180deg" : undefined }}
    />
  );
  return (
    <div style={cardColumn(2)}>
      {target ? (
        <Place
          target={target}
          targets={targets}
          onPick={panel.togglePicking}
          light={light}
          trailing={chevron}
        />
      ) : (
        <button
          type="button"
          onClick={panel.togglePicking}
          style={{
            display: "flex",
            alignItems: "center",
            gap: 8,
            padding: "7px 8px",
            border: 0,
            borderRadius: 8,
            background: "transparent",
            color: alertInk(0.6, ground),
            fontSize: 12,
            cursor: "pointer",
          }}
        >
          <span style={{ flex: 1, textAlign: "left" }}>
            {targets ? t("Choose where to send") : t("Looking for sessions…")}
          </span>
          {chevron}
        </button>
      )}
      {picking && targets && client && (
        <div
          style={{
            ...cardColumn(2),
            padding: 3,
            borderRadius: 10,
            background: light ? "rgba(0,0,0,0.03)" : "rgba(255,255,255,0.04)",
          }}
        >
          {clients.length > 1 && (
            <AgentTabs
              clients={clients}
              chosen={client}
              pick={panel.setTab}
              light={light}
            />
          )}
          <div style={{ maxHeight: LIST_HEIGHT, overflowY: "auto" }}>
            <FolderGroups
              sessions={targets.sessions.filter((row) => row.client === client)}
              start={targets.starts.find((start) => start.client === client)}
              targets={targets}
              target={target}
              near={near}
              pick={panel.choose}
              light={light}
            />
          </div>
        </div>
      )}
    </div>
  );
}
