import type { Provider, RelaySource, Snapshot } from "@agent-hud/protocol";
import {
  moneyAmount,
  snapshotLanguage,
  type Translate,
} from "@agent-hud/ui/format";
import { LinkButton } from "../SettingsControls";
import { Row, Section } from "../SettingsPrimitives";
import { useOpenSettingsPage } from "../SettingsNavigation";

/** The relays this agent goes through; they are set up on the Relays page. */
export function AgentRelays({
  t,
  snapshot,
  source,
}: {
  t: Translate;
  snapshot: Snapshot;
  source: Provider;
}) {
  const openPage = useOpenSettingsPage();
  const relays = (snapshot.relays ?? []).filter((relay) =>
    relay.agents.includes(source.id),
  );
  if (!relays.length) return null;
  return (
    <Section
      title={t("Relays")}
      action={
        openPage && (
          <LinkButton onClick={() => openPage("relays")}>
            {t("Relay settings")}
          </LinkButton>
        )
      }
    >
      {relays.map((relay) => (
        <Row
          key={relay.provider}
          title={relay.name ?? relay.provider}
          subtitle={relayBalance(t, snapshot, relay)}
        />
      ))}
    </Section>
  );
}

/** A relay's balance as its row says it, or why there is none. */
export function relayBalance(
  t: Translate,
  snapshot: Snapshot,
  relay: RelaySource,
): string {
  return relay.balance
    ? t("Balance {{amount}}", {
        amount: moneyAmount(
          relay.balance.amount,
          relay.balance.currency,
          snapshotLanguage(snapshot),
        ),
      })
    : !relay.consented
      ? t("Balance not read")
      : (relay.notice ?? t("Balance not read yet"));
}
