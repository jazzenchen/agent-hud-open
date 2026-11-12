import { vendorKey } from "@agent-hud/hud/model";
import type { Snapshot } from "@agent-hud/protocol";

/** Agents in the filter: those with tokens this year, plus any enabled
 * agent that has a quota or an account but no local token ledger. */
export function listedAgents(
  year:
    | { providers?: { id: string; name: string; selectedTokens: number }[] }
    | null
    | undefined,
  snapshot: Pick<Snapshot, "providers" | "quotas" | "accounts">,
): { id: string; name: string }[] {
  const listed = new Map<string, { id: string; name: string }>();
  for (const provider of year?.providers ?? []) {
    if (provider.selectedTokens > 0) {
      listed.set(vendorKey(provider.id), {
        id: provider.id,
        name: provider.name,
      });
    }
  }
  for (const provider of snapshot.providers ?? []) {
    const key = vendorKey(provider.id);
    if (listed.has(key) || provider.enabled === false) continue;
    const hasQuota = (snapshot.quotas ?? []).some(
      (quota) => vendorKey(quota.vendor) === key,
    );
    const hasAccount = (snapshot.accounts ?? []).some(
      (account) => vendorKey(account.provider) === key,
    );
    if (hasQuota || hasAccount) {
      listed.set(key, { id: provider.id, name: provider.name });
    }
  }
  return [...listed.values()];
}
