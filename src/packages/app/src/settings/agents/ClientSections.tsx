/* What a client page shows of its connection and its accounts: whether
 * Agent HUD's hooks are in its settings and the step the client still wants,
 * and the accounts of its agent, set on its vendor's page. */
import type { ClientRow, Provider, Snapshot } from "@agent-hud/protocol";
import { vendorKey } from "@agent-hud/hud/model";
import type { Translate } from "@agent-hud/ui/format";
import { LinkButton } from "../SettingsControls";
import { Pill, Row, Section } from "../SettingsPrimitives";
import { useOpenSettingsPage } from "../SettingsNavigation";

/** Agent HUD's hooks in the client's settings: in, not in, or off for every
 * client, with the step the client wants before it runs them. Nothing for a
 * client Agent HUD puts no hooks into. */
export function ClientHooks({
  t,
  snapshot,
  source,
}: {
  t: Translate;
  snapshot: Snapshot;
  source: Provider;
}) {
  const openPage = useOpenSettingsPage();
  const hooks = snapshot.connection.clientHooks;
  const entry = hooks?.clients?.find((client) => client.source === source.id);
  if (!entry) return null;
  const pill =
    hooks?.enabled === false ? (
      <Pill>{t("Off")}</Pill>
    ) : entry.installed ? (
      <Pill tone="green" dot>
        {t("Installed")}
      </Pill>
    ) : (
      <Pill>{t("Not installed")}</Pill>
    );
  return (
    <Section footer={entry.notice ?? undefined}>
      <Row title={t("Client hooks")}>
        {pill}
        {openPage && (
          <LinkButton onClick={() => openPage("agent-plugins")}>
            {t("Agent plugins")}
          </LinkButton>
        )}
      </Row>
    </Section>
  );
}

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
