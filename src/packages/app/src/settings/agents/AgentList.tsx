import type { ReactNode } from "react";
import { ChevronRight, CircleAlert } from "lucide-react";
import type {
  ClientRow,
  Snapshot,
  SourcePreferences,
} from "@agent-hud/protocol";
import { HUDArtwork as HUDLogo } from "@agent-hud/hud/HUDArtwork";
import type { Translate } from "@agent-hud/ui/format";
import { Collapsible, Section } from "../SettingsPrimitives";
import { featureName, features, uses } from "./features";

/** The agents: the ones found here, each with the features it uses and a
 * mark while it waits for a step in it; then the rest, folded in a card of
 * their own. The clients of one vendor sit under its name, which opens the
 * vendor's page. */
export function AgentList({
  t,
  snapshot,
  clients,
  preferences,
  select,
  selectGroup,
}: {
  t: Translate;
  snapshot: Snapshot;
  clients: ClientRow[];
  preferences: Record<string, SourcePreferences>;
  select: (id: string) => void;
  selectGroup: (group: string) => void;
}) {
  const found = clients.filter((client) => client.detected);
  const missing = clients.filter((client) => !client.detected);
  // An agent whose hooks wait for a step in it, such as Codex's review.
  const waiting = (client: ClientRow) =>
    snapshot.connection.clientHooks?.clients?.some(
      (entry) =>
        entry.source === client.agent && entry.installed && entry.notice,
    ) ?? false;
  const item = (client: ClientRow) => {
    const used = features.filter((feature) =>
      uses(client, feature, snapshot, preferences),
    );
    return (
      <button
        key={client.id}
        type="button"
        className={`agents-row${client.enabled ? "" : " is-off"}`}
        onClick={() => select(client.id)}
      >
        <HUDLogo source={client.agent} size={18} />
        <span className="agents-row-name">{client.name}</span>
        <span className="agents-row-tags">
          {!client.enabled ? (
            <span className="agents-row-note">{t("Off")}</span>
          ) : (
            used.map((feature) => (
              <span key={feature} className="agents-row-tag">
                {featureName(feature, t)}
              </span>
            ))
          )}
        </span>
        {client.enabled && waiting(client) && (
          <CircleAlert size={15} className="agents-row-waiting" />
        )}
        <ChevronRight size={14} className="agents-row-chevron" />
      </button>
    );
  };
  const head = (group: string, name: string) => (
    <button
      type="button"
      className="agents-row-group"
      onClick={() => selectGroup(group)}
    >
      {name}
      <ChevronRight size={12} />
    </button>
  );
  return (
    <Section title={t("Your agents")} plain>
      <div className="agents-rows" data-guide="agents-list">
        {grouped(found).map(([group, rows]) =>
          groupOf(group, rows, item, head),
        )}
      </div>
      {missing.length > 0 && (
        <div className="agents-missing">
          <Collapsible
            title={t("Not on this computer")}
            summary={missing.length}
          >
            <div className="agents-rows">
              {grouped(missing).map(([group, rows]) =>
                groupOf(group, rows, item, head),
              )}
            </div>
          </Collapsible>
        </div>
      )}
    </Section>
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
      className="agents-row-vendor"
      role="group"
      aria-label={name}
    >
      {head(group, name)}
      {rows.map(item)}
    </div>
  );
}
