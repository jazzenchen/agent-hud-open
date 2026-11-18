import type { ReactNode } from "react";
import { ChevronRight, CircleAlert } from "lucide-react";
import { vendorKey } from "@agent-hud/hud/model";
import type {
  ClientRow,
  Snapshot,
  SourcePreferences,
} from "@agent-hud/protocol";
import type { MCPClientsStatus } from "../agent-plugins/services";
import { AgentLogo } from "../AgentLogo";
import type { Translate } from "@agent-hud/ui/format";
import { Collapsible, Section } from "../SettingsPrimitives";
import { featureName, features, uses } from "./features";
import { hookStep, mergedFrom } from "./steps";

/** The agents: the ones found here, each with the features it uses and a
 * mark while it waits for a step in it (only one it reports, such as Codex's
 * hooks to allow); then the rest, folded in a card of
 * their own. The clients of one vendor sit under its name, which opens the
 * vendor's page and names the account its clients share, marked merged. */
export function AgentList({
  t,
  snapshot,
  clients,
  preferences,
  select,
  selectGroup,
  added,
}: {
  t: Translate;
  snapshot: Snapshot;
  clients: ClientRow[];
  preferences: Record<string, SourcePreferences>;
  select: (id: string) => void;
  selectGroup: (group: string) => void;
  /** Which clients have HUD tools, once read. */
  added: MCPClientsStatus | null;
}) {
  const found = clients.filter((client) => client.detected);
  const missing = clients.filter((client) => !client.detected);
  const item = (client: ClientRow) => {
    const used = features.filter((feature) =>
      uses(client, feature, snapshot, preferences, added),
    );
    // A step the agent waits for in it, such as Codex's hook review; its
    // words show on hover.
    const step = hookStep(snapshot, client.agent);
    return (
      <button
        key={client.id}
        type="button"
        className={`agents-row${client.enabled ? "" : " is-off"}`}
        onClick={() => select(client.id)}
      >
        <AgentLogo source={client.agent} size={18} />
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
        {client.enabled && step?.waiting && (
          <span
            className="agents-row-waiting"
            title={step.text}
            aria-label={step.text}
          >
            <CircleAlert size={15} />
          </span>
        )}
        <ChevronRight size={14} className="agents-row-chevron" />
      </button>
    );
  };
  const head = (group: string, name: string, rows: ClientRow[]) => (
    <button
      type="button"
      className="agents-row-group"
      onClick={() => selectGroup(group)}
    >
      {name}
      <ChevronRight size={12} />
      {shared(snapshot, rows).map(([account, from]) => (
        <span
          key={account}
          className="agents-row-merged"
          title={from.join(" + ")}
        >
          {account}
          <span className="agents-row-tag">{t("Merged")}</span>
        </span>
      ))}
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

/** The accounts the clients in `rows` share, each once by name with the
 * clients it was read through: one account signed in to in several of a
 * vendor's clients, merged. */
function shared(snapshot: Snapshot, rows: ClientRow[]): [string, string[]][] {
  const agents = new Set(rows.map((row) => vendorKey(row.agent)));
  const found = new Map<string, string[]>();
  for (const account of snapshot.accounts ?? []) {
    if (!agents.has(vendorKey(account.provider))) continue;
    const from = mergedFrom(snapshot, account);
    if (!from) continue;
    const name = account.name ?? account.label ?? account.id;
    found.set(name, [...new Set([...(found.get(name) ?? []), ...from])]);
  }
  return [...found];
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
  head: (group: string, name: string, rows: ClientRow[]) => ReactNode,
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
      {head(group, name, rows)}
      {rows.map(item)}
    </div>
  );
}
