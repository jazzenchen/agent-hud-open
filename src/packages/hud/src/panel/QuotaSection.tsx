import { memo, useMemo, useState } from "react";
import type { PanelBlockLayout, Quota, Snapshot } from "@agent-hud/protocol";
import { quotaMetricAvailability } from "../quota-metrics";
import { useFormat } from "@agent-hud/ui/format-context";
import { tokensShort } from "@agent-hud/ui/format";
import { quotaAccountKey, vendorKey } from "../model";
import { recentCost, recentUsage } from "../recent-usage";
import { AccountHeader, BlockNotice, WebSignInNotice } from "./AccountHeader";
import {
  missingReadingNotice,
  serviceNotice,
  webSignInOffer,
} from "./account-labels";
import { ResetCreditsRow } from "./ResetCreditsRow";
import { usePanelServices } from "../PanelContext";
import { useGround } from "../ground";
import { measures, palette, type Logo, type Measure } from "./theme";
import { SourceTokenChart } from "./TokenChartSection";
import { MetricPicker } from "./MetricPicker";
import { BlockHeading } from "./BlockHeading";
import { RingReset } from "./QuotaRingCard";
import { QuotaWidgets } from "./QuotaWidgets";

export const SourceProviderBlock = memo(function SourceProviderBlock({
  vendor,
  rows,
  now,
  light,
  Logo,
  showReset,
  accountInfo,
  layout = {},
  snapshot,
  active = true,
  onStats,
}: {
  vendor: string;
  rows: Quota[];
  now: number;
  light: boolean;
  Logo: Logo;
  showReset: boolean;
  accountInfo: Snapshot["accounts"];
  layout?: PanelBlockLayout;
  snapshot: Snapshot;
  active?: boolean;
  onStats?: () => void;
}) {
  const offered = layout.measures?.length
    ? layout.measures
    : measures.map((m) => m.key);
  const [chosen, setMeasure] = useState<Measure>(offered[0]);
  const measure = offered.includes(chosen) ? chosen : offered[0];
  const format = useFormat();
  const services = usePanelServices();
  const availability = useMemo(
    () =>
      Object.fromEntries(
        measures.map(({ key }) => [
          key,
          quotaMetricAvailability(rows, key, now, format),
        ]),
      ) as Record<Measure, ReturnType<typeof quotaMetricAvailability>>,
    [rows, now, format],
  );
  const colors = palette(useGround(light));
  // Open's ReportView.accountSections: the rows split by account in row
  // order. Only a section whose rows name an account the readers reported
  // gets a header; rows of no account stand without one.
  const sections = useMemo(() => {
    const order: string[] = [];
    const byKey = new Map<string, Quota[]>();
    for (const row of rows) {
      const key = quotaAccountKey(row);
      if (!byKey.has(key)) {
        order.push(key);
        byKey.set(key, []);
      }
      byKey.get(key)!.push(row);
    }
    return order.map((key) => {
      const sectionRows = byKey.get(key)!;
      const id = sectionRows[0].accountId;
      const account = id
        ? (accountInfo?.find((profile) => profile.id === id) ?? null)
        : null;
      return {
        key,
        rows: sectionRows,
        account,
        current: account?.current !== false,
      };
    });
  }, [rows, accountInfo]);
  const missingReading = !snapshot.quotas.some((row) => row.vendor === vendor);
  // Without readings the block still says why, as a signed-out Claude Code
  // asks to be signed in again.
  const missingNotice = missingReading
    ? missingReadingNotice(snapshot.providers, vendor)
    : null;
  const vendorStatus = serviceNotice(
    snapshot.providers,
    vendor,
    format.language,
  );
  // A quota only the vendor's site shows waits for the user to sign in.
  const signInOffer = webSignInOffer(snapshot.providers, vendor);
  const signInAgain =
    snapshot.providers.find((p) => p.id === vendor)?.accountStatus?.failure ===
    "signedOut";
  // Without a window, the block shows what the agent used in the last 24
  // hours instead (vendors.rs `recent_label`).
  const recent = missingReading
    ? recentUsage(snapshot, vendorKey(vendor))
    : null;
  const recentMoney = recent ? recentCost(recent, format.language) : null;
  const countdown = layout.countdown ?? showReset;
  // A ring with no legend makes a compact block: stacked, the account and the
  // reset under the ring.
  const compact =
    layout.legend === false &&
    ["rings", "segments", "ring"].includes(layout.widget ?? "");
  return (
    <section
      className="hud-source-provider"
      style={{
        display: "flex",
        flexDirection: "column",
        gap: "var(--hud-block-gap, 8px)",
      }}
    >
      <BlockHeading
        vendor={vendor}
        Logo={Logo}
        light={light}
        color={colors.text}
        compact={compact}
        picker={
          layout.widget !== "chart" &&
          !missingReading && (
            <MetricPicker
              selected={measure}
              choose={setMeasure}
              light={light}
              availability={
                Object.fromEntries(
                  Object.entries(availability).map(([key, value]) => [
                    key,
                    {
                      ...value,
                      selectable:
                        value.selectable && offered.includes(key as Measure),
                    },
                  ]),
                ) as typeof availability
              }
            />
          )
        }
      />
      {vendorStatus && (
        <BlockNotice text={vendorStatus} light={light} warning />
      )}
      {signInOffer && services?.webSignIn && (
        <WebSignInNotice
          sites={signInOffer.sites}
          again={signInAgain}
          light={light}
          signIn={(host) => services.webSignIn?.(signInOffer.provider, host)}
        />
      )}
      {layout.widget === "chart" ? (
        <SourceTokenChart
          snapshot={snapshot}
          focusVendor={vendor}
          light={light}
          Logo={Logo}
          layout={{ period: layout.period }}
          active={active}
        />
      ) : missingReading ? (
        <>
          {missingNotice && <BlockNotice text={missingNotice} light={light} />}
          {recent ? (
            <div
              className="hud-source-recent"
              title={
                recent.unpricedTokens > 0
                  ? format.language === "zh"
                    ? `其中 ${new Intl.NumberFormat("en").format(recent.unpricedTokens)} 个 Token 来自没有 API 价格的模型，未计入花费`
                    : `${new Intl.NumberFormat("en").format(recent.unpricedTokens)} of these tokens came from models with no API list price and are not in the cost`
                  : undefined
              }
              style={{
                display: "flex",
                justifyContent: "space-between",
                gap: 8,
                color: colors.secondary,
              }}
            >
              <span>
                {format.language === "zh" ? "最近 24 小时" : "Last 24 hours"}
              </span>
              <strong style={{ color: colors.text }}>
                {`${tokensShort(recent.tokens)} Token`}
                {recentMoney ? ` · ${recentMoney}` : ""}
              </strong>
            </div>
          ) : (
            <div
              style={{
                display: "flex",
                justifyContent: "space-between",
                color: colors.secondary,
              }}
            >
              <span>{format.language === "zh" ? "额度" : "Quota"}</span>
              <strong>--</strong>
            </div>
          )}
        </>
      ) : (
        sections.map(({ key, rows: sectionRows, account, current }) => {
          // Earned resets belong to the signed-in Codex account.
          const resets =
            account &&
            current &&
            sectionRows.some((row) => vendorKey(row.vendor) === "openai")
              ? account.resetCredits
              : null;
          return (
            <div
              key={key}
              className="hud-source-account-section"
              style={{
                display: "flex",
                flexDirection: "column",
                gap: "var(--hud-block-gap, 8px)",
              }}
            >
              {account && !compact && (
                <AccountHeader account={account} now={now} light={light} />
              )}
              <div
                style={{
                  display: "grid",
                  gap: "var(--hud-block-gap, 8px)",
                  opacity: current ? 1 : 0.55,
                }}
              >
                <QuotaWidgets
                  rows={sectionRows}
                  layout={layout}
                  measure={measure}
                  now={now}
                  light={light}
                  showReset={countdown}
                  onStats={onStats}
                />
              </div>
              {account && compact && (
                <AccountHeader
                  account={account}
                  now={now}
                  light={light}
                  compact
                />
              )}
              {compact && countdown && (
                <RingReset rows={sectionRows} now={now} light={light} />
              )}
              {resets && (
                <ResetCreditsRow
                  resets={resets}
                  showExpiry={countdown}
                  light={light}
                  compact={compact}
                />
              )}
            </div>
          );
        })
      )}
    </section>
  );
});
