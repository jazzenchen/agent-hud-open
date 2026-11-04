import { useState } from "react";
import { GripVertical } from "lucide-react";
import type { PanelBlockLayout, Quota, Snapshot } from "@agent-hud/protocol";
import { Switch } from "@agent-hud/ui/components/switch";
import {
  quotaAccountKey,
  quotaRowHidden,
  quotaRowKey,
  quotaRowMatches,
  withRowShown,
} from "@agent-hud/hud/model";
import { quotaPercentLabel } from "@agent-hud/ui/format";
import { Card, Row, type Text } from "../SettingsPrimitives";
import { moveAmong, ReorderButtons } from "./Reorder";

const dragType = "text/agent-hud-window";

/** One account's card: the account, then its quota windows, which can be
 * dragged or moved with the arrows within the account. */
export function AccountCard({
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
