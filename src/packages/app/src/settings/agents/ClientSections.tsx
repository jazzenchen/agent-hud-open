/* What a client page lists of its spending: the accounts and relays its own
 * calls went through. The accounts are set on its vendor's page, the relays
 * on the Relays page. */
import type { ClientRow, Snapshot } from "@agent-hud/protocol";
import { vendorKey } from "@agent-hud/hud/model";
import type { Translate } from "@agent-hud/ui/format";
import { LinkButton } from "../SettingsControls";
import { Row, Section } from "../SettingsPrimitives";
import { relayBalance } from "./AgentRelays";
import { mergedFrom } from "./steps";

/** What the client's calls went through in the last 30 days: its agent's
 * accounts, each with its plan, when it used the agent's own sign-in; the
 * relays, each with its balance; any other provider by its name. A desktop
 * app on a subscription and a command line on a relay each show their own.
 * Without calls, the agent's accounts. */
export function ClientAccounts({
  t,
  snapshot,
  client,
  openGroup,
}: {
  t: Translate;
  snapshot: Snapshot;
  client: ClientRow;
  openGroup: () => void;
}) {
  const spends = client.spends;
  const own = !spends || spends.own;
  const accounts = own
    ? (snapshot.accounts ?? []).filter(
        (account) => vendorKey(account.provider) === vendorKey(client.agent),
      )
    : [];
  return (
    <Section
      title={t("Accounts")}
      action={<LinkButton onClick={openGroup}>{client.groupName}</LinkButton>}
      footer={
        spends
          ? t("From this client's calls in the last 30 days.")
          : t("No calls from this client in the last 30 days.")
      }
    >
      {accounts.map((account) => {
        const merged = mergedFrom(snapshot, account);
        return (
          <Row
            key={account.id}
            title={account.name ?? account.label}
            tag={merged ? t("Merged") : undefined}
            subtitle={accountLine(account.planLabel ?? account.plan, merged)}
          />
        );
      })}
      {own && !accounts.length && (
        <Row muted title={t("No quota or balance read yet")} />
      )}
      {spends?.relays.map((host) => {
        const relay = snapshot.relays?.find((row) => row.provider === host);
        return (
          <Row
            key={`relay:${host}`}
            title={relay?.name ?? host}
            tag={t("Relay")}
            subtitle={relay && relayBalance(t, snapshot, relay)}
          />
        );
      })}
      {spends?.routes.map((route) => (
        <Row key={`route:${route}`} title={route} tag={t("Custom provider")} />
      ))}
    </Section>
  );
}

/** An account's line: its plan, and the clients it was read through when it
 * was merged from several. */
export function accountLine(
  plan: string | null | undefined,
  merged: string[] | null,
): string | undefined {
  return [plan, merged?.join(" + ")].filter(Boolean).join(" · ") || undefined;
}
