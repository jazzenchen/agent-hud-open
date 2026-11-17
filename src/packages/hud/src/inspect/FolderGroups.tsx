import { useState } from "react";
import { Folder, Plus } from "lucide-react";
import type {
  InspectSessionRow,
  InspectStart,
  InspectTarget,
  InspectTargets,
} from "@agent-hud/protocol";
import { useT } from "@agent-hud/ui/format-context";
import { cardColumn } from "../card-column";
import { useGround } from "../ground";
import { alertInk } from "../tokens";
import { folderName, newTarget, Place } from "./Place";

/** `sessions` by folder, in the order their newest session comes. */
export function byFolder(sessions: InspectSessionRow[]) {
  const groups = new Map<string, InspectSessionRow[]>();
  for (const row of sessions) {
    const folder = row.folder ?? "";
    groups.set(folder, [...(groups.get(folder) ?? []), row]);
  }
  return [...groups].map(([folder, rows]) => ({ folder, rows }));
}

/** A folder's line: its last path name, and when the client starts there,
 * a way to send to a new session in it. */
function FolderHeader({
  folder,
  opens,
  start,
  chosen,
  pick,
  light,
}: {
  folder: string;
  /** The folder as the start offer writes it. */
  opens: string | null;
  start?: InspectStart;
  chosen: boolean;
  pick: (target: InspectTarget) => void;
  light: boolean;
}) {
  const t = useT();
  const ground = useGround(light);
  const [hover, setHover] = useState(false);
  const starts = Boolean(start && opens && start.folders.includes(opens));
  return (
    <div
      title={folder || undefined}
      style={{
        display: "flex",
        alignItems: "center",
        gap: 6,
        padding: "5px 4px 1px 8px",
        fontSize: 10.5,
        fontWeight: 600,
        color: alertInk(0.5, ground),
      }}
    >
      <Folder size={11} strokeWidth={2} style={{ flexShrink: 0 }} />
      <span
        style={{
          flex: 1,
          minWidth: 0,
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
        }}
      >
        {folderName(folder) || t("No folder")}
      </span>
      {starts && start && (
        <button
          type="button"
          onClick={() =>
            pick({ kind: "new", client: start.client, folder: opens ?? "" })
          }
          onPointerEnter={() => setHover(true)}
          onPointerLeave={() => setHover(false)}
          style={{
            display: "inline-flex",
            alignItems: "center",
            gap: 3,
            flexShrink: 0,
            padding: "2px 6px",
            border: 0,
            borderRadius: 6,
            background:
              hover || chosen
                ? light
                  ? "rgba(0,0,0,0.06)"
                  : "rgba(255,255,255,0.09)"
                : "transparent",
            color: alertInk(chosen ? 0.9 : 0.55, ground),
            fontSize: 10.5,
            fontWeight: 500,
            cursor: "pointer",
          }}
        >
          <Plus size={10} strokeWidth={2.4} />
          {t("New session")}
        </button>
      )}
    </div>
  );
}

/** One client's sessions grouped by folder, newest folder first, each
 * folder by its last path name with its sessions under it. With no session
 * at all, the client's new session alone. */
export function FolderGroups({
  sessions,
  start,
  targets,
  target,
  near,
  pick,
  light,
}: {
  sessions: InspectSessionRow[];
  start?: InspectStart;
  targets: InspectTargets;
  target: InspectTarget | null;
  near: string | null | undefined;
  pick: (target: InspectTarget) => void;
  light: boolean;
}) {
  if (!sessions.length) {
    return start ? (
      <Place
        target={newTarget(start, near)}
        targets={targets}
        chosen={target?.kind === "new" && target.client === start.client}
        onPick={() => pick(newTarget(start, near))}
        light={light}
      />
    ) : null;
  }
  return (
    <div style={cardColumn(1)}>
      {byFolder(sessions).map(({ folder, rows }) => (
        <div key={folder} style={cardColumn(1)}>
          <FolderHeader
            folder={folder}
            opens={rows[0].startFolder}
            start={start}
            chosen={
              target?.kind === "new" &&
              target.client === start?.client &&
              target.folder === rows[0].startFolder
            }
            pick={pick}
            light={light}
          />
          {rows.map((row) => {
            const place: InspectTarget = {
              kind: "session",
              client: row.client,
              sessionId: row.id,
            };
            return (
              <Place
                key={row.id}
                target={place}
                targets={targets}
                grouped
                chosen={
                  target?.kind === "session" && target.sessionId === row.id
                }
                muted={row.reachable === false}
                onPick={() => row.reachable !== false && pick(place)}
                light={light}
              />
            );
          })}
        </div>
      ))}
    </div>
  );
}
