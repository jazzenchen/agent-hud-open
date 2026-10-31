import { Fragment, useState, type ReactNode } from "react";
import { Info } from "lucide-react";
import { Switch } from "@agent-hud/ui/components/switch";
import { moneyAmount, snapshotLanguage } from "@agent-hud/ui/format";
import type { RelaySource } from "@agent-hud/protocol";
import {
  LinkButton,
  Notice,
  Page,
  Row,
  Section,
  SmallButton,
  Unavailable,
  useText,
  type SettingsPaneProps,
} from "./SettingsPrimitives";
import { RelayForm, RelayKindForm } from "./RelayForms";

/**
 * The relay page: every relay once, by its address, whether a client's
 * configuration names it or the owner added it by hand. Each row says where
 * it was found, which agents use it, what kind of relay it is and what it has
 * left; its switch lets Agent HUD read the balance (a relay added by hand is
 * always read, and is removed instead). The type is chosen by hand only when
 * sniffing found none, or to change that choice.
 */
export function RelaysPane({ snapshot, action }: SettingsPaneProps) {
  const t = useText(snapshot);
  const language = snapshotLanguage(snapshot);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [adding, setAdding] = useState(false);
  // The relay address whose type is being chosen by hand.
  const [choosing, setChoosing] = useState<string | null>(null);
  const relays = snapshot.relays ?? [];
  const consented = snapshot.settings.relayReading ?? [];
  const agentName = (id: string) =>
    snapshot.providers.find((provider) => provider.id === id)?.name ?? id;
  const names = (ids: string[]) => ids.map(agentName).join(t(", ", "、"));
  const kinds: Record<string, string> = {
    newapi: "New API",
    sub2api: "Sub2API",
    openrouter: "OpenRouter",
    custom: t("Custom endpoint", "自定义接口"),
  };
  const perform = async (job: () => Promise<unknown>) => {
    setPending(true);
    setError(null);
    try {
      await job();
      return true;
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : String(failure));
      return false;
    } finally {
      setPending(false);
    }
  };
  /** Where the relay came from, next to its name. */
  const origin = (relay: RelaySource) => {
    if (relay.manual) return t("Added by hand", "手动添加");
    const clients = relay.clients ?? [];
    if (clients.length === 0) return undefined;
    return t(`Found in ${names(clients)}`, `来自 ${names(clients)}`);
  };
  /** Who uses it, beyond the clients its tag already names. */
  const users = (relay: RelaySource) => {
    const clients = relay.clients ?? [];
    const agents = relay.agents ?? [];
    if (relay.manual || clients.length === 0)
      return agents.length
        ? t(`Used by ${names(agents)}`, `${names(agents)} 在用`)
        : t("Not used yet", "还没有智能体在用");
    const others = agents.filter((agent) => !clients.includes(agent));
    return others.length
      ? t(`Also used by ${names(others)}`, `${names(others)} 也在用`)
      : undefined;
  };
  const kind = (relay: RelaySource) => {
    const shown = relay.chosen ?? relay.kind;
    if (!shown || !kinds[shown]) return undefined;
    return relay.chosen
      ? t(`${kinds[shown]} (chosen)`, `${kinds[shown]}（手选）`)
      : kinds[shown];
  };
  /** What it has left; nothing while reading it is off. A missing balance
   * keeps its reason on hover. */
  const balance = (relay: RelaySource, agreed: boolean): ReactNode => {
    if (!agreed) return undefined;
    if (relay.balance)
      return t(
        `Balance ${moneyAmount(relay.balance.amount, relay.balance.currency, language)}`,
        `余额 ${moneyAmount(relay.balance.amount, relay.balance.currency, language)}`,
      );
    if (relay.notice)
      return (
        <Unavailable reason={relay.notice}>
          {t("No balance", "未读到余额")}
          <Info className="ml-1 size-3 self-center" aria-hidden />
        </Unavailable>
      );
    return t("Balance not read yet", "余额还没读到");
  };
  const facts = (relay: RelaySource, agreed: boolean) =>
    [
      relay.name ? relay.provider : undefined,
      users(relay),
      kind(relay),
      balance(relay, agreed),
    ]
      .filter(Boolean)
      .map((part, index) => (
        <Fragment key={index}>
          {index > 0 && " · "}
          {part}
        </Fragment>
      ));
  return (
    <Page>
      {error && <Notice tone="error">{error}</Notice>}
      <Section
        action={
          // The header's action slot is muted for counts and notes; the
          // page's main button keeps the normal text colour.
          <span className="contents text-foreground">
            <SmallButton
              disabled={pending || adding}
              onClick={() => setAdding(true)}
            >
              {t("Add a relay", "添加中转站")}
            </SmallButton>
          </span>
        }
        footer={
          relays.some((relay) => !relay.manual)
            ? t(
                "Turn a relay on to read its balance with its own key. The key stays on this computer and goes only to that relay.",
                "打开开关即用中转站自己的 Key 读取余额。Key 只留在本机，只发给这个中转站。",
              )
            : t(
                "Balances are read with each relay's own key. The key stays on this computer and goes only to that relay.",
                "余额用中转站自己的 Key 读取。Key 只留在本机，只发给这个中转站。",
              )
        }
      >
        {adding && (
          <RelayForm
            t={t}
            pending={pending}
            onCancel={() => setAdding(false)}
            onSave={(relay) =>
              void perform(() => action("add_relay_source", relay)).then(
                (saved) => {
                  if (saved) setAdding(false);
                },
              )
            }
          />
        )}
        {relays.map((relay) => {
          const agreed = relay.manual || consented.includes(relay.provider);
          // The type is chosen by hand only when sniffing found none, or to
          // change a choice made before.
          const choosable = (relay.unrecognized || relay.chosen) && agreed;
          return (
            <Fragment key={relay.provider}>
              <Row
                title={relay.name ?? relay.provider}
                tag={origin(relay)}
                detail={facts(relay, agreed)}
              >
                {choosable && choosing !== relay.provider && (
                  <LinkButton
                    disabled={pending}
                    onClick={() => setChoosing(relay.provider)}
                  >
                    {relay.chosen
                      ? t("Change type", "更改类型")
                      : t("Choose type", "选择类型")}
                  </LinkButton>
                )}
                {relay.manual ? (
                  <SmallButton
                    disabled={pending}
                    onClick={() =>
                      void perform(() =>
                        action("remove_relay_source", { id: relay.id }),
                      )
                    }
                  >
                    {t("Remove", "移除")}
                  </SmallButton>
                ) : (
                  <Switch
                    checked={agreed}
                    disabled={pending}
                    aria-label={t(
                      `Read ${relay.provider}'s balance`,
                      `读取 ${relay.provider} 的余额`,
                    )}
                    onCheckedChange={(value) =>
                      void perform(() =>
                        action("set_setting", {
                          key: "relayReading",
                          value: value
                            ? [...consented, relay.provider]
                            : consented.filter(
                                (host) => host !== relay.provider,
                              ),
                        }),
                      )
                    }
                  />
                )}
              </Row>
              {choosing === relay.provider && (
                <RelayKindForm
                  t={t}
                  pending={pending}
                  kind={relay.chosen ?? "auto"}
                  custom={relay.custom}
                  onCancel={() => setChoosing(null)}
                  onSave={(choice) =>
                    void perform(() =>
                      action("set_relay_kind", {
                        ...choice,
                        host: relay.provider,
                      }),
                    ).then((saved) => {
                      if (saved) setChoosing(null);
                    })
                  }
                />
              )}
            </Fragment>
          );
        })}
        {relays.length === 0 && !adding && (
          <Row muted title={t("No relays yet", "还没有中转站")} />
        )}
      </Section>
    </Page>
  );
}
