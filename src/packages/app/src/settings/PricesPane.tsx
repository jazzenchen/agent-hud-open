import { Fragment, useCallback, useEffect, useState } from "react";
import { HUDArtwork as HUDLogo } from "@agent-hud/hud/HUDArtwork";
import { useT } from "@agent-hud/ui/format-context";
import {
  moneyAmount,
  snapshotLanguage,
  tokensShort,
} from "@agent-hud/ui/format";
import type { PriceList, PriceRow } from "@agent-hud/protocol";
import {
  Collapsible,
  Notice,
  Pill,
  Row,
  Section,
  Page,
  type PillTone,
  type SettingsPaneProps,
} from "./SettingsPrimitives";
import { LinkButton, Segments } from "./SettingsControls";
import { MultiplierInput, PriceEditor } from "./PriceFields";
import { usePending } from "./use-pending";

/**
 * The price page: every model used in the last 30 days, folded under the
 * agent that used it, with what it costs per million tokens and where that
 * comes from (the vendor's list, a relay's own list, a multiplier, the
 * owner's price, or free on this computer). A group opens by itself when it
 * has a model nothing prices. Under them, the fallbacks: each relay's
 * multiplier and what each local address is.
 */
export function PricesPane({ snapshot, action }: SettingsPaneProps) {
  const t = useT();
  const language = snapshotLanguage(snapshot);
  const [list, setList] = useState<PriceList | null>(null);
  const [editing, setEditing] = useState<string | null>(null);
  const { pending, error, setError, run } = usePending();
  const load = useCallback(
    () =>
      action("price_list")
        .then((answer) => setList(answer as PriceList))
        .catch((failure: unknown) =>
          setError(
            failure instanceof Error ? failure.message : String(failure),
          ),
        ),
    [action],
  );
  // Read again when the ledger changes, so the 30-day token counts stay live.
  useEffect(() => {
    void load();
  }, [load, snapshot.statisticsRevision]);
  const save = (key: string, value: unknown) =>
    run(async () => {
      await action("set_setting", { key, value });
      await load();
    });
  const customPrices = snapshot.settings.customPrices ?? {};
  const multipliers = snapshot.settings.relayMultipliers ?? {};
  const localKinds = snapshot.settings.localKinds ?? {};
  const origins: Record<string, [string, PillTone]> = {
    catalog: [t("List price"), "gray"],
    relay: [t("Relay's list"), "gray"],
    multiplier: [t("Multiplier"), "gray"],
    custom: [t("Custom"), "blue"],
    local: [t("Local"), "green"],
  };
  const origin = (row: PriceRow): [string, PillTone] =>
    row.price
      ? (origins[row.price.origin] ?? [row.price.origin, "gray"])
      : [t("Unpriced"), "orange"];
  const money = (value: number, currency: string) =>
    moneyAmount(value, currency, language, true);
  /** Tokens in 30 days, then the input and output rates. */
  const facts = (row: PriceRow) => {
    const tokens = t("{{tokens}} tokens", { tokens: tokensShort(row.tokens) });
    if (!row.price) return tokens;
    if (row.price.origin === "local") return t("{{tokens}} · Free", { tokens });
    const input = money(row.price.input, row.price.currency);
    const output = money(row.price.output, row.price.currency);
    return t("{{tokens}} · {{input}} in · {{output}} out", {
      tokens,
      input,
      output,
    });
  };
  // The agents in the order the list names them, each with its models.
  const groups = new Map<string, PriceRow[]>();
  for (const row of list?.models ?? [])
    groups.set(row.client, [...(groups.get(row.client) ?? []), row]);
  const modelRow = (row: PriceRow) => {
    const [label, tone] = origin(row);
    const custom = customPrices[row.model] ?? null;
    return (
      <Fragment key={row.id}>
        <Row title={row.name} detail={facts(row)}>
          <Pill tone={tone}>{label}</Pill>
          {editing !== row.id && (
            <LinkButton disabled={pending} onClick={() => setEditing(row.id)}>
              {custom ? t("Edit price") : t("Set price")}
            </LinkButton>
          )}
        </Row>
        {editing === row.id && (
          <PriceEditor
            row={row}
            custom={custom}
            pending={pending}
            t={t}
            onSave={async (price) => {
              const next = { ...customPrices };
              if (price) next[row.model] = price;
              else delete next[row.model];
              if (await save("customPrices", next)) setEditing(null);
            }}
            onCancel={() => setEditing(null)}
          />
        )}
      </Fragment>
    );
  };
  return (
    <Page>
      {error && <Notice tone="error">{error}</Notice>}
      {list === null && !error && (
        <Section>
          <Row muted title={t("Reading prices…")} />
        </Section>
      )}
      {list && list.models.length === 0 && (
        <Section>
          <Row muted title={t("No model used in the last 30 days")} />
        </Section>
      )}
      {groups.size > 0 && (
        <Section
          plain
          footer={t("Models used in the last 30 days, per million tokens.")}
        >
          {[...groups].map(([client, rows]) => {
            const unpriced = rows.filter((row) => !row.price).length;
            return (
              <Collapsible
                key={client}
                icon={<HUDLogo source={rows[0].source} size={18} />}
                title={client}
                defaultOpen={unpriced > 0}
                summary={
                  <>
                    <span className="settings-row-subtitle">
                      {t("{{count}} models", { count: rows.length })}
                    </span>
                    {unpriced > 0 && (
                      <Pill tone="orange">
                        {t("{{number}} unpriced", { number: unpriced })}
                      </Pill>
                    )}
                  </>
                }
              >
                {rows.map(modelRow)}
              </Collapsible>
            );
          })}
        </Section>
      )}
      {list && list.relays.length > 0 && (
        <Section
          title={t("Relay multipliers")}
          subtitle={t(
            "List price × multiplier, for relays without a price list.",
          )}
        >
          {list.relays.map((relay) => (
            <Row
              key={relay.host}
              title={relay.host}
              detail={
                relay.listed
                  ? t("Uses its own price list")
                  : t("No price list found")
              }
            >
              <MultiplierInput
                label={t("{{host}} multiplier", { host: relay.host })}
                value={multipliers[relay.host]}
                disabled={pending}
                placeholder={t("e.g. 0.3")}
                onCommit={(times) => {
                  const next = { ...multipliers };
                  if (times === null) delete next[relay.host];
                  else next[relay.host] = times;
                  void save("relayMultipliers", next);
                }}
              />
            </Row>
          ))}
        </Section>
      )}
      {list && list.localHosts.length > 0 && (
        <Section
          title={t("Addresses on this computer")}
          subtitle={t("A model server is free; a gateway uses list prices.")}
          footer={t(
            "Automatic counts a model with a list price as a gateway call.",
          )}
        >
          {list.localHosts.map((local) => (
            <Row key={local.host} title={local.host}>
              <Segments
                label={local.host}
                value={localKinds[local.host] ?? "auto"}
                disabled={pending}
                options={[
                  ["auto", t("Automatic")],
                  ["model", t("Model server")],
                  ["gateway", t("Gateway")],
                ]}
                change={(kind) => {
                  const next = { ...localKinds };
                  if (kind === "auto") delete next[local.host];
                  else next[local.host] = kind as "model" | "gateway";
                  void save("localKinds", next);
                }}
              />
            </Row>
          ))}
        </Section>
      )}
    </Page>
  );
}
