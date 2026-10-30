import { Fragment, useCallback, useEffect, useState } from "react";
import { HUDArtwork as HUDLogo } from "@agent-hud/hud/HUDArtwork";
import {
  moneyAmount,
  snapshotLanguage,
  tokensShort,
} from "@agent-hud/ui/format";
import type { PriceList, PriceRow } from "@agent-hud/protocol";
import {
  Collapsible,
  LinkButton,
  Notice,
  Pill,
  Row,
  Section,
  Segments,
  Page,
  useText,
  type PillTone,
  type SettingsPaneProps,
} from "./SettingsPrimitives";
import { MultiplierInput, PriceEditor } from "./PriceFields";

/**
 * The price page: every model used in the last 30 days, folded under the
 * agent that used it, with what it costs per million tokens and where that
 * comes from (the vendor's list, a relay's own list, a multiplier, the
 * owner's price, or free on this computer). A group opens by itself when it
 * has a model nothing prices. Under them, the fallbacks: each relay's
 * multiplier and what each local address is.
 */
export function PricesPane({ snapshot, action }: SettingsPaneProps) {
  const t = useText(snapshot);
  const language = snapshotLanguage(snapshot);
  const [list, setList] = useState<PriceList | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [editing, setEditing] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
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
  useEffect(() => {
    void load();
  }, [load]);
  const save = async (key: string, value: unknown) => {
    setPending(true);
    setError(null);
    try {
      await action("set_setting", { key, value });
      await load();
      return true;
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : String(failure));
      return false;
    } finally {
      setPending(false);
    }
  };
  const customPrices = snapshot.settings.customPrices ?? {};
  const multipliers = snapshot.settings.relayMultipliers ?? {};
  const localKinds = snapshot.settings.localKinds ?? {};
  const origins: Record<string, [string, PillTone]> = {
    catalog: [t("List price", "价目表"), "gray"],
    relay: [t("Relay's list", "中转站价格表"), "gray"],
    multiplier: [t("Multiplier", "倍率"), "gray"],
    custom: [t("Custom", "自定义"), "blue"],
    local: [t("Local", "本地"), "green"],
  };
  const origin = (row: PriceRow): [string, PillTone] =>
    row.price
      ? (origins[row.price.origin] ?? [row.price.origin, "gray"])
      : [t("Unpriced", "未计价"), "orange"];
  const money = (value: number, currency: string) =>
    moneyAmount(value, currency, language, true);
  /** Tokens in 30 days, then the input and output rates. */
  const facts = (row: PriceRow) => {
    const tokens = t(
      `${tokensShort(row.tokens)} tokens`,
      `${tokensShort(row.tokens)} Token`,
    );
    if (!row.price) return tokens;
    if (row.price.origin === "local") return `${tokens} · ${t("Free", "免费")}`;
    const input = money(row.price.input, row.price.currency);
    const output = money(row.price.output, row.price.currency);
    return `${tokens} · ${t(`${input} in · ${output} out`, `输入 ${input} · 输出 ${output}`)}`;
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
              {custom
                ? t("Edit price", "修改价格")
                : t("Set price", "设置价格")}
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
          <Row muted title={t("Reading prices…", "正在读取价格…")} />
        </Section>
      )}
      {list && list.models.length === 0 && (
        <Section>
          <Row
            muted
            title={t(
              "No model used in the last 30 days",
              "最近 30 天没有用过模型",
            )}
          />
        </Section>
      )}
      {groups.size > 0 && (
        <Section
          plain
          footer={t(
            "Models used in the last 30 days, per million tokens.",
            "最近 30 天用过的模型，按每百万 Token 计价。",
          )}
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
                      {rows.length === 1
                        ? t("1 model", "1 个模型")
                        : t(`${rows.length} models`, `${rows.length} 个模型`)}
                    </span>
                    {unpriced > 0 && (
                      <Pill tone="orange">
                        {t(`${unpriced} unpriced`, `${unpriced} 个未计价`)}
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
          title={t("Relay multipliers", "中转站倍率")}
          subtitle={t(
            "List price × multiplier, for relays without a price list.",
            "没有价格表的中转站，按官方价 × 倍率计价。",
          )}
        >
          {list.relays.map((relay) => (
            <Row
              key={relay.host}
              title={relay.host}
              detail={
                relay.listed
                  ? t("Uses its own price list", "按它自己的价格表计价")
                  : t("No price list found", "没有读到价格表")
              }
            >
              <MultiplierInput
                label={t(`${relay.host} multiplier`, `${relay.host} 倍率`)}
                value={multipliers[relay.host]}
                disabled={pending}
                placeholder={t("e.g. 0.3", "如 0.3")}
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
          title={t("Addresses on this computer", "本机地址")}
          subtitle={t(
            "A model server is free; a gateway uses list prices.",
            "本地模型免费；本地网关按官方价计价。",
          )}
          footer={t(
            "Automatic counts a model with a list price as a gateway call.",
            "自动：价目表里有的模型按网关算，其他按本地模型算。",
          )}
        >
          {list.localHosts.map((local) => (
            <Row key={local.host} title={local.host}>
              <Segments
                label={local.host}
                value={localKinds[local.host] ?? "auto"}
                disabled={pending}
                options={[
                  ["auto", t("Automatic", "自动")],
                  ["model", t("Model server", "本地模型")],
                  ["gateway", t("Gateway", "本地网关")],
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
