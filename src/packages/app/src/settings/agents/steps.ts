// What a client asks of the user apart from Agent HUD, and how the clients
// that read one account are named.
import type { Account, Snapshot } from "@agent-hud/protocol";

/** The step an agent asks for before it runs Agent HUD's hooks (Codex's
 * review, TRAE's global hooks), or that it gets none on this system; and
 * whether it waits for it now. Only a wait the agent reports counts: Codex
 * names the hooks it skips until they are allowed. A step Agent HUD cannot
 * check is told on the agent's page, never marked. */
export function hookStep(
  snapshot: Snapshot,
  agent: string,
): { text: string; waiting: boolean } | null {
  const text = snapshot.connection.clientHooks?.clients?.find(
    (entry) => entry.source === agent && (entry.installed || entry.unsupported),
  )?.notice;
  if (!text) return null;
  const skipped =
    snapshot.providers.find((provider) => provider.id === agent)?.accountStatus
      ?.unreviewedHooks ?? [];
  return { text, waiting: skipped.length > 0 };
}

/** The clients an account was read through, by name in the agent list's
 * order, when it was more than one: one account signed in to in a command
 * line and a desktop app shows once, marked merged. */
export function mergedFrom(
  snapshot: Snapshot,
  account: Account | undefined,
): string[] | null {
  const readers = account?.readBy ?? [];
  if (readers.length < 2) return null;
  const clients = snapshot.clients ?? [];
  const place = (id: string) => {
    const at = clients.findIndex((client) => client.id === id);
    return at < 0 ? clients.length : at;
  };
  return [...readers]
    .sort((a, b) => place(a) - place(b))
    .map((id) => clients.find((client) => client.id === id)?.name ?? id);
}
