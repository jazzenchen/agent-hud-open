import { useState, type ReactNode } from "react";
import type {
  InspectSessionRow,
  InspectStart,
  InspectTarget,
  InspectTargets,
} from "@agent-hud/protocol";
import type { Translate } from "@agent-hud/ui/format";
import { useT } from "@agent-hud/ui/format-context";
import { HUDArtwork } from "../HUDArtwork";
import { clientDisplayName } from "../IslandNews";
import { cardColumn } from "../card-column";
import { useGround } from "../ground";
import { alertInk } from "../tokens";

/** A folder as the list names it: its last path name. */
export const folderName = (path: string | null | undefined) =>
  path ? (path.split(/[\\/]/).filter(Boolean).pop() ?? path) : "";

export function stateLabel(row: InspectSessionRow, t: Translate) {
  if (row.reachable === false) return row.reason;
  if (row.state === "running") return t("Goes in when this turn ends");
  if (row.state === "waitingForApproval") return t("Waiting");
  return "";
}

/** A new session's place: `near` when this client starts there, else its
 * newest folder, else a folder of its own. */
export function newTarget(
  start: InspectStart,
  near: string | null | undefined,
): InspectTarget {
  const folder =
    near && start.folders.includes(near) ? near : (start.folders[0] ?? "");
  return { kind: "new", client: start.client, folder };
}

/** What a place reads as: its client, its title, and a line under it. */
function describe(
  target: InspectTarget,
  targets: InspectTargets | null,
  t: Translate,
  grouped?: boolean,
) {
  const name = clientDisplayName(target.client);
  if (target.kind === "new") {
    return {
      title: t("New {{client}} session", { client: name }),
      detail: target.folder
        ? t("in {{folder}}", { folder: folderName(target.folder) })
        : t("in a folder of its own"),
    };
  }
  const row = targets?.sessions.find(
    (session) =>
      session.client === target.client && session.id === target.sessionId,
  );
  return {
    title: row?.title || t("Untitled session"),
    detail: row
      ? [grouped ? "" : folderName(row.folder), stateLabel(row, t)]
          .filter(Boolean)
          .join(" · ")
      : "",
  };
}

/** One place to send to. Under its folder in the list (`grouped`) it shows
 * neither its client nor its folder, which the tab and the folder say. */
export function Place({
  target,
  targets,
  chosen,
  muted,
  grouped,
  onPick,
  light,
  trailing,
}: {
  target: InspectTarget;
  targets: InspectTargets | null;
  chosen?: boolean;
  muted?: boolean;
  grouped?: boolean;
  onPick: () => void;
  light: boolean;
  trailing?: ReactNode;
}) {
  const t = useT();
  const ground = useGround(light);
  const [hover, setHover] = useState(false);
  const { title, detail } = describe(target, targets, t, grouped);
  return (
    <button
      type="button"
      onClick={onPick}
      onPointerEnter={() => setHover(true)}
      onPointerLeave={() => setHover(false)}
      title={detail || undefined}
      style={{
        display: "flex",
        alignItems: "center",
        gap: 8,
        minWidth: 0,
        padding: grouped ? "3px 8px 3px 24px" : "6px 8px",
        border: 0,
        borderRadius: 8,
        background:
          hover || chosen
            ? light
              ? "rgba(0,0,0,0.05)"
              : "rgba(255,255,255,0.07)"
            : "transparent",
        color: "inherit",
        textAlign: "left",
        cursor: "pointer",
        opacity: muted ? 0.45 : 1,
      }}
    >
      {!grouped && (
        <span style={{ display: "inline-flex", flexShrink: 0, opacity: 0.85 }}>
          <HUDArtwork source={target.client} size={14} light={light} />
        </span>
      )}
      <span style={{ ...cardColumn(1), flex: 1 }}>
        <span
          style={{
            fontSize: 12,
            fontWeight: 500,
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}
        >
          {title}
        </span>
        {detail && (
          <span
            style={{
              fontSize: 10.5,
              color: alertInk(0.5, ground),
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
            }}
          >
            {detail}
          </span>
        )}
      </span>
      {trailing}
    </button>
  );
}
