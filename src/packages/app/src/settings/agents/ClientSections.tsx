/* The accounts a client page lists: its agent's, set on its vendor's page. */
import type { ClientRow, Snapshot } from "@agent-hud/protocol";
import { vendorKey } from "@agent-hud/hud/model";
import type { Translate } from "@agent-hud/ui/format";
import { LinkButton } from "../SettingsControls";
import { Row, Section } from "../SettingsPrimitives";

/** The accounts the client's agent reads, each with its plan; they are set
 * on its vendor's page. */
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
  const accounts = (snapshot.accounts ?? []).filter(
    (account) => vendorKey(account.provider) === vendorKey(client.agent),
  );
  return (
    <Section
      title={t("Accounts")}
      action={<LinkButton onClick={openGroup}>{client.groupName}</LinkButton>}
    >
      {accounts.map((account) => (
        <Row
          key={account.id}
          title={account.name ?? account.label}
          tag={account.planLabel ?? account.plan ?? undefined}
        />
      ))}
      {!accounts.length && (
        <Row muted title={t("No quota or balance read yet")} />
      )}
    </Section>
  );
}
