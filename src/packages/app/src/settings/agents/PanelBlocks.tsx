/* "In the panel": what of one agent the hover panel shows on the screen chosen
 * on the Display page: its block and the block's place, its accounts, quota
 * windows and balances. Everything here is written to that screen's panel
 * layout. */
import { useState } from "react";
import { GripVertical } from "lucide-react";
import type {
  PanelBlockLayout,
  PanelLayout,
  Provider,
  Quota,
  Snapshot,
} from "@agent-hud/protocol";
import { Switch } from "@agent-hud/ui/components/switch";
import {
  inRowOrder,
  quotaAccountKey,
  quotaRowHidden,
  quotaRowKey,
  quotaRowMatches,
  vendorKey,
  withRowShown,
} from "@agent-hud/hud/model";
import {
  moneyAmount,
  quotaPercentLabel,
  snapshotLanguage,
} from "@agent-hud/ui/format";
import { Card, Row, Section, type Text } from "../SettingsPrimitives";
import { moveAmong, ReorderButtons } from "./Reorder";

const dragType = "text/agent-hud-window";

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

/** One account's card: the account, then its quota windows, which can be
 * dragged or moved with the arrows within the account. */
function AccountCard({
  t,
  snapshot,
  account,
  accounts,
  profile,
  windows,
  windowOrder,
  block,
  changeBlock,
  pending,
}: {
  t: Text;
  snapshot: Snapshot;
  account: string;
  accounts: string[];
  profile?: NonNullable<Snapshot["accounts"]>[number];
  windows: Quota[];
  windowOrder: string[];
  block: PanelBlockLayout;
  changeBlock: (patch: Partial<PanelBlockLayout>) => Promise<boolean>;
  pending: boolean;
}) {
  const [target, setTarget] = useState<string | null>(null);
  const accountWindows = windows.filter(
    (quota) => quotaAccountKey(quota) === account,
  );
  const ordered = windowOrder
    .map((id) => accountWindows.find((quota) => quotaRowMatches(quota, id)))
    .filter((quota): quota is Quota => Boolean(quota));
  const ids = ordered.map(quotaRowKey);
  const name =
    profile?.name ??
    accountWindows[0]?.account ??
    profile?.label ??
    t("Unknown account", "未知账户");
  const checked =
    !block.hidden &&
    !(block.hiddenAccounts ?? []).includes(account) &&
    (!block.accounts?.length || block.accounts.includes(account));

  const showAccount = (show: boolean) => {
    let selected = block.hidden
      ? []
      : block.accounts?.length
        ? [...block.accounts]
        : [...accounts];
    selected = show
      ? [...new Set([...selected, account])]
      : selected.filter((id) => id !== account);
    void changeBlock({
      accounts: block.accounts?.length ? selected : undefined,
      hiddenAccounts: show
        ? (block.hiddenAccounts ?? []).filter((id) => id !== account)
        : [...new Set([...(block.hiddenAccounts ?? []), account])],
      hidden: false,
    });
  };

  return (
    <Card>
      <Row
        title={name}
        subtitle={profile?.planLabel ?? profile?.plan ?? undefined}
      >
        <Switch
          checked={checked}
          disabled={pending}
          aria-label={`${t("Show account", "显示账户")} ${name}`}
          onCheckedChange={showAccount}
        />
      </Row>
      {ordered.map((window, index) => {
        const id = quotaRowKey(window);
        return (
          <div
            key={id}
            className={`agents-draggable${target === id ? " is-target" : ""}`}
            draggable={!pending}
            onDragStart={(event) => event.dataTransfer.setData(dragType, id)}
            onDragOver={(event) => {
              event.preventDefault();
              setTarget(id);
            }}
            onDragLeave={() => setTarget(null)}
            onDragEnd={() => setTarget(null)}
            onDrop={(event) => {
              event.preventDefault();
              setTarget(null);
              const from = event.dataTransfer.getData(dragType);
              // Windows move only within their own account.
              if (from === id || !ids.includes(from)) return;
              const values = windowOrder.filter((key) => key !== from);
              values.splice(values.indexOf(id), 0, from);
              void changeBlock({ rows: values });
            }}
          >
            <Row
              icon={<GripVertical size={13} className="agents-grip" />}
              title={window.name}
              tag={quotaPercentLabel(
                window.usedPercent,
                snapshot.settings.quotaDisplay,
              )}
            >
              <ReorderButtons
                t={t}
                name={window.name}
                first={index === 0}
                last={index === ordered.length - 1}
                disabled={pending}
                move={(direction) =>
                  void changeBlock({
                    rows: moveAmong(windowOrder, ids, id, direction),
                  })
                }
              />
              <Switch
                checked={!quotaRowHidden(window, block.hiddenRows ?? [])}
                disabled={pending}
                aria-label={`${t("Show", "显示")} ${window.name}`}
                onCheckedChange={(show) =>
                  void changeBlock({
                    hiddenRows: show
                      ? withRowShown(block.hiddenRows ?? [], window, windows)
                      : [...new Set([...(block.hiddenRows ?? []), id])],
                  })
                }
              />
            </Row>
          </div>
        );
      })}
    </Card>
  );
}
