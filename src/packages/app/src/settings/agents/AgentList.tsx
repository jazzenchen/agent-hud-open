import type { ReactNode } from "react";
import type { ClientRow } from "@agent-hud/protocol";
import { HUDArtwork as HUDLogo } from "@agent-hud/hud/HUDArtwork";
import type { Translate } from "@agent-hud/ui/format";

/** The clients, found ones first; the rest under a small label, still
 * selectable so they can be set up before they are installed. Within each
 * part, the clients of one vendor sit together under its name. */
export function AgentList({
  t,
  clients,
  selected,
  selectedGroup,
  select,
  selectGroup,
}: {
  t: Translate;
  clients: ClientRow[];
  selected?: string;
  /** The vendor whose page is shown, when it is. */
  selectedGroup?: string;
  select: (id: string) => void;
  selectGroup: (group: string) => void;
}) {
  const found = clients.filter((client) => client.detected);
  const missing = clients.filter((client) => !client.detected);
  const item = (client: ClientRow) => (
    <button
      key={client.id}
      type="button"
      className={`agents-list-item${client.id === selected ? " is-selected" : ""}${client.enabled ? "" : " is-off"}`}
      aria-current={client.id === selected ? "true" : undefined}
      onClick={() => select(client.id)}
    >
      <HUDLogo source={client.agent} size={18} />
      <span className="agents-list-name">{client.name}</span>
      {!client.enabled && <span className="agents-list-state">{t("Off")}</span>}
    </button>
  );
  // A vendor's name opens its page, where its clients' accounts are set.
  const head = (group: string, name: string) => (
    <button
      type="button"
      className={`agents-list-group-name${group === selectedGroup ? " is-selected" : ""}`}
      aria-current={group === selectedGroup ? "true" : undefined}
      onClick={() => selectGroup(group)}
    >
      {name}
    </button>
  );
  return (
    <nav
      className="agents-list"
      data-guide="agents-list"
      aria-label={t("Agents")}
    >
      {grouped(found).map(([group, rows]) => groupOf(group, rows, item, head))}
      {missing.length > 0 && (
        <>
          <div className="agents-list-label">{t("Not on this computer")}</div>
          {grouped(missing).map(([group, rows]) =>
            groupOf(group, rows, item, head),
          )}
        </>
      )}
    </nav>
  );
}

/** The rows by vendor, each vendor where its first client is. */
function grouped(rows: ClientRow[]): [string, ClientRow[]][] {
  const groups = new Map<string, ClientRow[]>();
  for (const row of rows) {
    const key = row.group || row.id;
    groups.set(key, [...(groups.get(key) ?? []), row]);
  }
  return [...groups];
}

/** A vendor's clients, under its name when it has more than one client. */
function groupOf(
  group: string,
  rows: ClientRow[],
  item: (client: ClientRow) => ReactNode,
  head: (group: string, name: string) => ReactNode,
) {
  const name = rows[0]?.groupName;
  if (!name) return rows.map(item);
  return (
    <div
      key={group}
      className="agents-list-group"
      role="group"
      aria-label={name}
    >
      {head(group, name)}
      {rows.map(item)}
    </div>
  );
}
