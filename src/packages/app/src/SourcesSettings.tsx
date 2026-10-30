import { useState, type CSSProperties } from "react";
import { ChevronDown, ChevronUp, RefreshCw } from "lucide-react";
import type {
  NativeMethod,
  PanelLayout,
  PanelBlockLayout,
  Snapshot,
  SourcePreferences,
} from "@agent-hud/protocol";
import { Button } from "@agent-hud/ui/components/button";
import { Switch } from "@agent-hud/ui/components/switch";
import { Input } from "@agent-hud/ui/components/input";
import CodexAccountHomes from "./settings/CodexAccountHomes";
import AntigravityAccounts from "./settings/AntigravityAccounts";
import { HUDArtwork as HUDLogo } from "@agent-hud/hud/HUDArtwork";
import {
  vendorKey,
  inRowOrder,
  quotaAccountKey,
  quotaRowHidden,
  quotaRowKey,
  quotaRowMatches,
  withRowShown,
} from "@agent-hud/hud/model";
import {
  moneyAmount,
  snapshotLanguage,
  quotaPercentLabel,
} from "@agent-hud/ui/format";
import { useOpenSettingsPage } from "./settings/SettingsNavigation";
interface Props {
  snapshot: Snapshot;
  action: (
    method: NativeMethod,
    params?: Record<string, unknown>,
  ) => Promise<unknown>;
  layout: PanelLayout;
  onLayout: (layout: PanelLayout) => Promise<unknown>;
  preferences: Record<string, SourcePreferences>;
  onPreferences: (source: string, patch: SourcePreferences) => Promise<unknown>;
}
const box: CSSProperties = {
  border: "1px solid var(--border)",
  background: "var(--card)",
  borderRadius: 12,
  overflow: "hidden",
};
const row: CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: 12,
  padding: "12px 16px",
};
export default function SourcesSettings({
  snapshot,
  action,
  layout,
  onLayout,
  preferences,
  onPreferences,
}: Props) {
  const [selectedID, setSelected] = useState<string>();
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [credentials, setCredentials] = useState<Record<string, string>>({});
  const openPage = useOpenSettingsPage();
  const zh = snapshotLanguage(snapshot) === "zh",
    t = (en: string, cn: string) => (zh ? cn : en);
  const providerOrder = (layout.order ?? []).map((key) =>
    key.replace(/^api:/, ""),
  );
  const providers = [...snapshot.providers].sort(
    (a, b) =>
      (providerOrder.includes(a.id) ? providerOrder.indexOf(a.id) : Infinity) -
        (providerOrder.includes(b.id)
          ? providerOrder.indexOf(b.id)
          : Infinity) || Number(b.detected) - Number(a.detected),
  );
  const source = providers.find((p) => p.id === selectedID) ?? providers[0];
  const windows = snapshot.quotas.filter(
    (q) => source && vendorKey(q.vendor) === vendorKey(source.id),
  );
  const billing = snapshot.billing.filter(
    (account) => source && vendorKey(account.vendor) === vendorKey(source.id),
  );
  const key =
    windows[0]?.vendor ??
    (billing[0] ? `api:${billing[0].vendor}` : (source?.id ?? ""));
  const block = layout.blocks?.[key] ?? {};
  const balanceKey = billing[0] ? `api:${billing[0].vendor}` : "";
  const balanceBlock = layout.blocks?.[balanceKey] ?? {};
  const profiles = (snapshot.accounts ?? []).filter(
    (account) => source && vendorKey(account.provider) === vendorKey(source.id),
  );
  const accounts = [...new Set(windows.map(quotaAccountKey))];
  // The relays this agent uses; they are set up on the Relays page.
  const relays = (snapshot.relays ?? []).filter(
    (relay) => source && relay.agents.includes(source.id),
  );
  const fields = (
    (source?.accountStatus as unknown as { configurableFields?: string[] })
      ?.configurableFields ?? []
  ).map((key) => ({
    key,
    label:
      (
        {
          apiKey: "API key",
          baseUrl: "API endpoint",
          organization: "Organization",
          project: "Project",
        } as Record<string, string>
      )[key] ?? key,
    secret: key === "apiKey",
    required: key === "apiKey",
  }));
  async function perform(job: () => Promise<unknown>) {
    setPending(true);
    setError(null);
    try {
      await job();
      return true;
    } catch (error) {
      setError(String(error));
      return false;
    } finally {
      setPending(false);
    }
  }
  const changeBlock = (patch: Partial<PanelBlockLayout>) =>
    perform(() =>
      onLayout({
        ...layout,
        blocks: { ...layout.blocks, [key]: { ...block, ...patch } },
      }),
    );
  const changeBalanceBlock = (patch: Partial<PanelBlockLayout>) =>
    perform(() =>
      onLayout({
        ...layout,
        blocks: {
          ...layout.blocks,
          [balanceKey]: { ...balanceBlock, ...patch },
        },
      }),
    );
  const move = (values: string[], id: string, direction: number) => {
    const at = values.indexOf(id),
      to = at + direction;
    if (at < 0 || to < 0 || to >= values.length) return values;
    const next = [...values];
    [next[at], next[to]] = [next[to], next[at]];
    return next;
  };
  const availableKeys = [
    ...new Set([
      ...snapshot.quotas.map((q) => q.vendor),
      ...snapshot.billing.map((account) => `api:${account.vendor}`),
    ]),
  ];
  const order = [
    ...(layout.order ?? []),
    ...availableKeys.filter((id) => !(layout.order ?? []).includes(id)),
  ];
  // Each row once, by the key a layout writes for it, wherever the layout's
  // order names it by any of its keys.
  const windowOrder = [
    ...new Set(inRowOrder(windows, block.rows ?? []).map(quotaRowKey)),
  ];
  // Keep absent IDs in the order so their position survives a temporary failed read.
  const balanceOrder = [
    ...new Set([...(balanceBlock.rows ?? []), ...billing.map((row) => row.id)]),
  ];
  const visibleBalanceOrder = balanceOrder.filter((id) =>
    billing.some((row) => row.id === id),
  );
  const moveBalance = (id: string, direction: number) => {
    const at = visibleBalanceOrder.indexOf(id);
    const adjacent = visibleBalanceOrder[at + direction];
    if (at < 0 || !adjacent) return balanceOrder;
    const next = [...balanceOrder];
    const left = next.indexOf(id),
      right = next.indexOf(adjacent);
    [next[left], next[right]] = [next[right], next[left]];
    return next;
  };
  return (
    <div
      className="source-settings"
      style={{
        display: "flex",
        gap: 20,
        alignItems: "flex-start",
        width: "100%",
      }}
    >
      <aside style={{ width: 185, flexShrink: 0, display: "grid", gap: 3 }}>
        {providers.map((provider) => (
          <button
            key={provider.id}
            onClick={() => {
              setSelected(provider.id);
              setCredentials({});
              setError(null);
            }}
            style={{
              display: "flex",
              gap: 10,
              alignItems: "center",
              textAlign: "left",
              padding: "10px 12px",
              border: 0,
              borderRadius: 8,
              color: "inherit",
              background:
                provider.id === source?.id ? "var(--accent)" : "transparent",
            }}
          >
            <HUDLogo source={provider.id} size={19} />
            <span style={{ flex: 1 }}>{provider.name}</span>
            <span
              aria-label={
                provider.detected
                  ? t("Detected", "已检测")
                  : t("Not detected", "未检测")
              }
              style={{
                width: 5,
                height: 5,
                borderRadius: 9,
                background: provider.detected
                  ? "#39c88a"
                  : "var(--muted-foreground)",
              }}
            />
          </button>
        ))}
      </aside>
      {source && (
        <div style={{ flex: 1, minWidth: 0, display: "grid", gap: 22 }}>
          <header style={{ display: "flex", alignItems: "center", gap: 10 }}>
            <HUDLogo source={source.id} size={30} />
            <h2 style={{ fontSize: 20, margin: 0, flex: 1 }}>{source.name}</h2>
            <Button
              variant="ghost"
              size="icon-sm"
              disabled={pending}
              onClick={() => void perform(() => action("refresh"))}
            >
              <RefreshCw size={15} />
            </Button>
          </header>
          <section style={box}>
            <div style={row}>
              <div style={{ flex: 1 }}>
                <strong>{t("Enabled", "启用")}</strong>
                <p
                  style={{
                    fontSize: 12,
                    color: "var(--muted-foreground)",
                    margin: "4px 0 0",
                  }}
                >
                  {source.notice}
                </p>
              </div>
              <Switch
                checked={source.enabled}
                disabled={pending}
                onCheckedChange={(enabled) =>
                  void perform(() =>
                    action("set_provider", { id: source.id, enabled }),
                  )
                }
              />
            </div>
            <div style={{ ...row, borderTop: "1px solid var(--border)" }}>
              <span style={{ flex: 1 }}>{t("Live status", "实时状态")}</span>
              <Switch
                checked={preferences[source.id]?.live !== false}
                disabled={pending}
                onCheckedChange={(live) =>
                  void perform(() => onPreferences(source.id, { live }))
                }
              />
            </div>
            <div style={{ ...row, borderTop: "1px solid var(--border)" }}>
              <span style={{ flex: 1 }}>
                {t("Completion reminders", "完成提醒")}
              </span>
              <Switch
                checked={preferences[source.id]?.reminders !== false}
                disabled={pending}
                onCheckedChange={(reminders) =>
                  void perform(() => onPreferences(source.id, { reminders }))
                }
              />
            </div>
          </section>
          {source.id === "codex" && (
            <CodexAccountHomes
              homes={snapshot.settings.codexHomes ?? []}
              zh={zh}
              onSave={(homes) =>
                action("set_setting", { key: "codexHomes", value: homes })
              }
            />
          )}
          {source.id === "antigravity" && (
            <AntigravityAccounts
              settings={snapshot.settings.antigravityCliSettings ?? []}
              zh={zh}
              action={action}
            />
          )}
          <section>
            <h3 style={{ fontSize: 14 }}>
              {t("Expanded panel on this display", "此屏幕的展开面板")}
            </h3>
            <div style={box}>
              <div style={row}>
                <span style={{ flex: 1 }}>
                  {t("Show this block", "显示此分组")}
                </span>
                <Switch
                  checked={!block.hidden}
                  disabled={pending}
                  onCheckedChange={(shown) =>
                    void changeBlock({ hidden: !shown })
                  }
                />
                <Button
                  size="icon-xs"
                  variant="ghost"
                  disabled={pending || order.indexOf(key) <= 0}
                  onClick={() =>
                    void perform(() =>
                      onLayout({ ...layout, order: move(order, key, -1) }),
                    )
                  }
                >
                  <ChevronUp size={13} />
                </Button>
                <Button
                  size="icon-xs"
                  variant="ghost"
                  disabled={
                    pending ||
                    order.indexOf(key) < 0 ||
                    order.indexOf(key) === order.length - 1
                  }
                  onClick={() =>
                    void perform(() =>
                      onLayout({ ...layout, order: move(order, key, 1) }),
                    )
                  }
                >
                  <ChevronDown size={13} />
                </Button>
              </div>
              {accounts.map((account) => {
                const checked =
                  !block.hidden &&
                  !(block.hiddenAccounts ?? []).includes(account) &&
                  (!block.accounts?.length || block.accounts.includes(account));
                const profile = profiles.find(
                  (profile) => profile.id === account,
                );
                const accountWindows = windows.filter(
                  (q) => quotaAccountKey(q) === account,
                );
                return (
                  <div
                    key={account || "unknown"}
                    style={{ borderTop: "1px solid var(--border)" }}
                  >
                    <div style={row}>
                      <div style={{ flex: 1, minWidth: 0 }}>
                        <strong style={{ fontSize: 13 }}>
                          {profile?.name ??
                            accountWindows[0]?.account ??
                            profile?.label ??
                            t("Account unavailable", "账户未知")}
                        </strong>
                        <p
                          style={{
                            margin: "3px 0 0",
                            fontSize: 12,
                            color: "var(--muted-foreground)",
                          }}
                        >
                          {profile?.planLabel ??
                            profile?.plan ??
                            t("Plan unavailable", "套餐未知")}
                        </p>
                      </div>
                      <Switch
                        checked={checked}
                        disabled={pending}
                        aria-label={t("Show account", "显示账户")}
                        onCheckedChange={(show) => {
                          let selected = block.hidden
                            ? []
                            : block.accounts?.length
                              ? [...block.accounts]
                              : [...accounts];
                          selected = show
                            ? [...new Set([...selected, account])]
                            : selected.filter((id) => id !== account);
                          void changeBlock({
                            accounts: block.accounts?.length
                              ? selected
                              : undefined,
                            hiddenAccounts: show
                              ? (block.hiddenAccounts ?? []).filter(
                                  (id) => id !== account,
                                )
                              : [
                                  ...new Set([
                                    ...(block.hiddenAccounts ?? []),
                                    account,
                                  ]),
                                ],
                            hidden: false,
                          });
                        }}
                      />
                    </div>
                    {windowOrder
                      .map((id) =>
                        accountWindows.find((q) => quotaRowMatches(q, id)),
                      )
                      .filter((q): q is NonNullable<typeof q> => Boolean(q))
                      .map((window) => (
                        <div
                          key={quotaRowKey(window)}
                          draggable={!pending}
                          onDragStart={(event) =>
                            event.dataTransfer.setData(
                              "text/agent-hud-window",
                              quotaRowKey(window),
                            )
                          }
                          onDragOver={(event) => event.preventDefault()}
                          onDrop={(event) => {
                            event.preventDefault();
                            const from = event.dataTransfer.getData(
                              "text/agent-hud-window",
                            );
                            if (!windowOrder.includes(from)) return;
                            const values = windowOrder.filter(
                              (id) => id !== from,
                            );
                            values.splice(
                              values.indexOf(quotaRowKey(window)),
                              0,
                              from,
                            );
                            void changeBlock({ rows: values });
                          }}
                          style={{
                            ...row,
                            padding: "9px 16px 9px 30px",
                            fontSize: 12,
                            cursor: "grab",
                          }}
                        >
                          <span style={{ flex: 1 }}>{window.name}</span>
                          <span style={{ color: "var(--muted-foreground)" }}>
                            {quotaPercentLabel(
                              window.usedPercent,
                              snapshot.settings.quotaDisplay,
                            )}
                          </span>
                          <Switch
                            checked={
                              !quotaRowHidden(window, block.hiddenRows ?? [])
                            }
                            disabled={pending}
                            aria-label={t("Show quota window", "显示额度窗口")}
                            onCheckedChange={(show) =>
                              void changeBlock({
                                hiddenRows: show
                                  ? withRowShown(
                                      block.hiddenRows ?? [],
                                      window,
                                      windows,
                                    )
                                  : [
                                      ...new Set([
                                        ...(block.hiddenRows ?? []),
                                        quotaRowKey(window),
                                      ]),
                                    ],
                              })
                            }
                          />
                          <Button
                            size="icon-xs"
                            variant="ghost"
                            disabled={
                              pending ||
                              windowOrder.indexOf(quotaRowKey(window)) === 0
                            }
                            onClick={() =>
                              void changeBlock({
                                rows: move(
                                  windowOrder,
                                  quotaRowKey(window),
                                  -1,
                                ),
                              })
                            }
                          >
                            <ChevronUp size={12} />
                          </Button>
                          <Button
                            size="icon-xs"
                            variant="ghost"
                            disabled={
                              pending ||
                              windowOrder.indexOf(quotaRowKey(window)) ===
                                windowOrder.length - 1
                            }
                            onClick={() =>
                              void changeBlock({
                                rows: move(windowOrder, quotaRowKey(window), 1),
                              })
                            }
                          >
                            <ChevronDown size={12} />
                          </Button>
                        </div>
                      ))}
                  </div>
                );
              })}
              {billing.length > 0 && (
                <div style={{ borderTop: "1px solid var(--border)" }}>
                  <div
                    style={{
                      ...row,
                      fontSize: 12,
                      color: "var(--muted-foreground)",
                    }}
                  >
                    {t("Balances", "余额")}
                  </div>
                  {balanceOrder
                    .map((id) => billing.find((item) => item.id === id))
                    .filter((item): item is NonNullable<typeof item> =>
                      Boolean(item),
                    )
                    .map((item) => (
                      <div
                        key={item.id}
                        style={{
                          ...row,
                          padding: "9px 16px 9px 30px",
                          fontSize: 12,
                        }}
                      >
                        <span style={{ flex: 1 }}>
                          {item.label} · {item.currency}
                        </span>
                        <span style={{ color: "var(--muted-foreground)" }}>
                          {moneyAmount(
                            item.amount,
                            item.currency,
                            snapshotLanguage(snapshot),
                          )}
                        </span>
                        <Switch
                          checked={
                            !(balanceBlock.hiddenRows ?? []).includes(item.id)
                          }
                          disabled={pending}
                          aria-label={
                            t("Show balance", "显示余额") + ` ${item.currency}`
                          }
                          onCheckedChange={(show) =>
                            void changeBalanceBlock({
                              hiddenRows: show
                                ? (balanceBlock.hiddenRows ?? []).filter(
                                    (id) => id !== item.id,
                                  )
                                : [
                                    ...new Set([
                                      ...(balanceBlock.hiddenRows ?? []),
                                      item.id,
                                    ]),
                                  ],
                            })
                          }
                        />
                        <Button
                          size="icon-xs"
                          variant="ghost"
                          disabled={
                            pending ||
                            visibleBalanceOrder.indexOf(item.id) === 0
                          }
                          onClick={() =>
                            void changeBalanceBlock({
                              rows: moveBalance(item.id, -1),
                            })
                          }
                          aria-label={t("Move balance up", "上移余额")}
                        >
                          <ChevronUp size={12} />
                        </Button>
                        <Button
                          size="icon-xs"
                          variant="ghost"
                          disabled={
                            pending ||
                            visibleBalanceOrder.indexOf(item.id) ===
                              visibleBalanceOrder.length - 1
                          }
                          onClick={() =>
                            void changeBalanceBlock({
                              rows: moveBalance(item.id, 1),
                            })
                          }
                          aria-label={t("Move balance down", "下移余额")}
                        >
                          <ChevronDown size={12} />
                        </Button>
                      </div>
                    ))}
                </div>
              )}
              {!windows.length && !billing.length && (
                <p
                  style={{
                    ...row,
                    fontSize: 12,
                    color: "var(--muted-foreground)",
                  }}
                >
                  {t(
                    "No quota windows are currently available. Recorded usage and live sessions remain independent.",
                    "当前没有可用额度窗口；用量记录与实时会话独立显示。",
                  )}
                </p>
              )}
            </div>
          </section>
          {source.id === "copilot" && (
            <section style={box}>
              <div style={row}>
                <div style={{ flex: 1 }}>
                  <strong>
                    {t("Read GitHub Copilot quota", "读取 GitHub Copilot 额度")}
                  </strong>
                  <p
                    style={{
                      fontSize: 12,
                      color: "var(--muted-foreground)",
                      margin: "4px 0 0",
                    }}
                  >
                    {t(
                      "Use this device's signed-in GitHub credentials to query Copilot quota.",
                      "使用此设备上已登录的 GitHub 凭据查询 Copilot 额度。",
                    )}
                  </p>
                </div>
                <Switch
                  checked={snapshot.settings.readCopilotQuota === true}
                  disabled={pending}
                  onCheckedChange={(value) =>
                    void perform(() =>
                      action("set_setting", { key: "readCopilotQuota", value }),
                    )
                  }
                />
              </div>
            </section>
          )}
          {relays.length > 0 && (
            <section>
              <h3 style={{ fontSize: 14 }}>{t("Relays", "中转站")}</h3>
              <div style={{ ...box, display: "grid" }}>
                {relays.map((relay) => (
                  <div key={relay.provider} style={row}>
                    <div style={{ flex: 1, minWidth: 0 }}>
                      <strong style={{ fontSize: 13 }}>
                        {relay.name ?? relay.provider}
                      </strong>
                      <p
                        style={{
                          fontSize: 12,
                          color: "var(--muted-foreground)",
                          margin: "2px 0 0",
                        }}
                      >
                        {relay.balance
                          ? t("Balance ", "余额 ") +
                            moneyAmount(
                              relay.balance.amount,
                              relay.balance.currency,
                              snapshotLanguage(snapshot),
                            )
                          : !relay.consented
                            ? t("Its balance is not read", "未开启读取余额")
                            : (relay.notice ??
                              t("Balance not read yet", "余额还没读到"))}
                      </p>
                    </div>
                    {openPage && (
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={() => openPage("relays")}
                      >
                        {t("Relay settings", "中转站设置")}
                      </Button>
                    )}
                  </div>
                ))}
              </div>
            </section>
          )}
          {fields.length > 0 && (
            <section>
              <h3 style={{ fontSize: 14 }}>{t("Connection", "连接配置")}</h3>
              <form
                style={{ ...box, padding: 16, display: "grid", gap: 12 }}
                onSubmit={(event) => {
                  event.preventDefault();
                  void perform(() =>
                    action("set_credentials", {
                      provider: source.id,
                      credentials,
                    }),
                  ).then((saved) => {
                    if (saved) setCredentials({});
                  });
                }}
              >
                {fields.map((field) => (
                  <label
                    key={field.key}
                    style={{ display: "grid", gap: 5, fontSize: 12 }}
                  >
                    {field.label}
                    <Input
                      type={field.secret ? "password" : "text"}
                      autoComplete="off"
                      required={field.required}
                      value={credentials[field.key] ?? ""}
                      onChange={(event) =>
                        setCredentials((previous) => ({
                          ...previous,
                          [field.key]: event.target.value,
                        }))
                      }
                    />
                  </label>
                ))}
                <div
                  style={{
                    display: "flex",
                    gap: 8,
                    justifyContent: "flex-end",
                  }}
                >
                  <Button
                    type="button"
                    variant="outline"
                    disabled={pending}
                    onClick={() =>
                      void perform(() =>
                        action("remove_credentials", { provider: source.id }),
                      )
                    }
                  >
                    {t("Remove", "移除")}
                  </Button>
                  <Button disabled={pending} type="submit">
                    {t("Save", "保存")}
                  </Button>
                </div>
              </form>
            </section>
          )}
          {error && (
            <p
              role="alert"
              style={{ color: "var(--destructive)", fontSize: 12 }}
            >
              {error}
            </p>
          )}
        </div>
      )}
    </div>
  );
}
