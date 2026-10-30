import type { Provider, Snapshot } from "@agent-hud/protocol";
import { moneyAmount, snapshotLanguage } from "@agent-hud/ui/format";
import { LinkButton, Row, Section, type Text } from "../SettingsPrimitives";
import { useOpenSettingsPage } from "../SettingsNavigation";

/** The relays this agent goes through; they are set up on the Relays page. */
export function AgentRelays({
  t,
  snapshot,
  source,
}: {
  t: Text;
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
      title={t("Relays", "中转站")}
      action={
        openPage && (
          <LinkButton onClick={() => openPage("relays")}>
            {t("Relay settings", "中转站设置")}
          </LinkButton>
        )
      }
    >
      {relays.map((relay) => (
        <Row
          key={relay.provider}
          title={relay.name ?? relay.provider}
          subtitle={
            relay.balance
              ? t("Balance ", "余额 ") +
                moneyAmount(
                  relay.balance.amount,
                  relay.balance.currency,
                  snapshotLanguage(snapshot),
                )
              : !relay.consented
                ? t("Balance not read", "未开启读取余额")
                : (relay.notice ?? t("Balance not read yet", "余额还没读到"))
          }
        />
      ))}
    </Section>
  );
}
