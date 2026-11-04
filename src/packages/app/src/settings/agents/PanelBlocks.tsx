/* "In the panel": what of one agent the hover panel shows on the screen chosen
 * on the Display page: its block and the block's place, its accounts, quota
 * windows and balances. Everything here is written to that screen's panel
 * layout. */
import type {
  PanelBlockLayout,
  PanelLayout,
  Provider,
  Snapshot,
} from "@agent-hud/protocol";
import { Switch } from "@agent-hud/ui/components/switch";
import {
  inRowOrder,
  quotaAccountKey,
  quotaRowKey,
  vendorKey,
} from "@agent-hud/hud/model";
import { moneyAmount, snapshotLanguage } from "@agent-hud/ui/format";
import { Card, Row, Section, type Text } from "../SettingsPrimitives";
import { AccountCard } from "./AccountCard";
import { moveAmong, ReorderButtons } from "./Reorder";

export function PanelBlocks({
  t,
  snapshot,
  source,
  layout,
  onLayout,
  perform,
  pending,
}: {
  t: Text;
  snapshot: Snapshot;
  source: Provider;
  layout: PanelLayout;
  onLayout: (layout: PanelLayout) => Promise<unknown>;
  perform: (job: () => Promise<unknown>) => Promise<boolean>;
  pending: boolean;
}) {
  const same = (vendor: string) => vendorKey(vendor) === vendorKey(source.id);
  const windows = snapshot.quotas.filter((quota) => same(quota.vendor));
  const billing = snapshot.billing.filter((account) => same(account.vendor));
  // The block a quota-less, balance-less agent still has in the panel is
  // named by the agent's id.
  const key =
    windows[0]?.vendor ?? (billing[0] ? `api:${billing[0].vendor}` : source.id);
  const block = layout.blocks?.[key] ?? {};
  const balanceKey = billing[0] ? `api:${billing[0].vendor}` : "";
  const balanceBlock = layout.blocks?.[balanceKey] ?? {};
  const profiles = (snapshot.accounts ?? []).filter((account) =>
    same(account.provider),
  );
  const accounts = [...new Set(windows.map(quotaAccountKey))];

  const changeBlockAt = (id: string, patch: Partial<PanelBlockLayout>) =>
    perform(() =>
      onLayout({
        ...layout,
        blocks: {
          ...layout.blocks,
          [id]: { ...(layout.blocks?.[id] ?? {}), ...patch },
        },
      }),
    );
  const changeBlock = (patch: Partial<PanelBlockLayout>) =>
    changeBlockAt(key, patch);
  const changeBalanceBlock = (patch: Partial<PanelBlockLayout>) =>
    changeBlockAt(balanceKey, patch);

  // The panel's block order, with blocks it has not placed yet at the end.
  const placed = layout.order ?? [];
  const availableKeys = [
    ...new Set([
      ...snapshot.quotas.map((quota) => quota.vendor),
      ...snapshot.billing.map((account) => `api:${account.vendor}`),
    ]),
  ];
  const order = [
    ...placed,
    ...availableKeys.filter((id) => !placed.includes(id)),
  ];

  // Each window once, by the key a layout writes for it, wherever the
  // layout's order names it by any of its keys.
  const windowOrder = [
    ...new Set(inRowOrder(windows, block.rows ?? []).map(quotaRowKey)),
  ];
  // Absent balances keep their place, so it survives a failed read.
  const balanceOrder = [
    ...new Set([...(balanceBlock.rows ?? []), ...billing.map((row) => row.id)]),
  ];
  const visibleBalanceOrder = balanceOrder.filter((id) =>
    billing.some((row) => row.id === id),
  );
  const balances = visibleBalanceOrder
    .map((id) => billing.find((item) => item.id === id))
    .filter((item): item is NonNullable<typeof item> => Boolean(item));

  const blockRow = (
    id: string,
    title: string,
    name: string,
    hidden: boolean | undefined,
    change: (patch: Partial<PanelBlockLayout>) => unknown,
  ) => {
    const at = order.indexOf(id);
    return (
      <Row title={title}>
        <ReorderButtons
          t={t}
          name={name}
          first={at <= 0}
          last={at < 0 || at === order.length - 1}
          disabled={pending}
          move={(direction) =>
            void perform(() =>
              onLayout({
                ...layout,
                order: moveAmong(order, order, id, direction),
              }),
            )
          }
        />
        <Switch
          aria-label={title}
          checked={!hidden}
          disabled={pending}
          onCheckedChange={(shown) => void change({ hidden: !shown })}
        />
      </Row>
    );
  };

  const balanceRows = balances.map((item, index) => {
    const amount = moneyAmount(
      item.amount,
      item.currency,
      snapshotLanguage(snapshot),
    );
    return (
      <Row key={item.id} title={item.label} tag={amount}>
        <ReorderButtons
          t={t}
          name={`${item.label} ${amount}`}
          first={index === 0}
          last={index === balances.length - 1}
          disabled={pending}
          move={(direction) =>
            void changeBalanceBlock({
              rows: moveAmong(
                balanceOrder,
                visibleBalanceOrder,
                item.id,
                direction,
              ),
            })
          }
        />
        <Switch
          checked={!(balanceBlock.hiddenRows ?? []).includes(item.id)}
          disabled={pending}
          aria-label={`${t("Show balance", "显示余额")} ${item.label} ${amount}`}
          onCheckedChange={(show) =>
            void changeBalanceBlock({
              hiddenRows: show
                ? (balanceBlock.hiddenRows ?? []).filter((id) => id !== item.id)
                : [...new Set([...(balanceBlock.hiddenRows ?? []), item.id])],
            })
          }
        />
      </Row>
    );
  });

  return (
    <Section
      plain
      title={t("In the panel", "面板中显示")}
      footer={t(
        "Applies to the screen chosen on the Display page.",
        "作用于“显示”页面选中的屏幕。",
      )}
    >
      <Card>
        {blockRow(
          key,
          t("Show in panel", "在面板中显示"),
          source.name,
          block.hidden,
          changeBlock,
        )}
        {balanceKey === key && balanceRows}
        {!windows.length && !billing.length && (
          <Row
            muted
            title={t("No quota or balance read yet", "还没有读到额度或余额")}
          />
        )}
      </Card>
      {accounts.map((account) => (
        <AccountCard
          key={account || "unknown"}
          t={t}
          snapshot={snapshot}
          account={account}
          accounts={accounts}
          profile={profiles.find((profile) => profile.id === account)}
          windows={windows}
          windowOrder={windowOrder}
          block={block}
          changeBlock={changeBlock}
          pending={pending}
        />
      ))}
      {balanceKey && balanceKey !== key && (
        <Card>
          {blockRow(
            balanceKey,
            t("Balances", "余额"),
            `${source.name} ${t("balances", "余额")}`,
            balanceBlock.hidden,
            changeBalanceBlock,
          )}
          {balanceRows}
        </Card>
      )}
    </Section>
  );
}
