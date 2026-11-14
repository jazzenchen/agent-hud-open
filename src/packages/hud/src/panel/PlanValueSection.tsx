import type {
  PanelLayout,
  PlanValueAccount,
  PlanValueResponse,
  Snapshot,
} from "@agent-hud/protocol";
import { useFormat, useT } from "@agent-hud/ui/format-context";
import { listCost, listPrice, type Translate } from "@agent-hud/ui/format";
import { usePanelQuery } from "../PanelContext";
import { useGround } from "../ground";
import { palette, tabular, type Logo } from "./theme";
import { AddedSectionHeading, SectionNote } from "./AddedSectionParts";
import { agentShown } from "./added-sections";

const ellipsis = {
  overflow: "hidden",
  textOverflow: "ellipsis",
  whiteSpace: "nowrap",
} as const;

/** The value as a multiple of the plan's price, "≥" when it is a lower bound. */
function ratioText(account: PlanValueAccount, t: Translate): string {
  if (account.ratio == null) return "—";
  const figure =
    account.ratio < 10
      ? account.ratio.toFixed(1)
      : String(Math.round(account.ratio));
  return `${account.partial ? "≥" : ""}${t("{{figure}}×", { figure })}`;
}

/** The hover panel's plan value: each signed-in plan beside what the last 30
 * days of its calls on this Mac would cost at API list prices. The layout's
 * hidden models are left out of the value, as the Statistics window's are. */
export function PlanValueSection({
  snapshot,
  focusVendor,
  light,
  Logo,
  layout = {},
  active = true,
  bare = false,
}: {
  snapshot: Snapshot;
  focusVendor?: string;
  light: boolean;
  Logo?: Logo;
  layout?: PanelLayout["planValue"];
  active?: boolean;
  bare?: boolean;
}) {
  const { language } = useFormat();
  const t = useT();
  const colors = palette(useGround(light));
  const query = usePanelQuery<PlanValueResponse>(
    "plan_value",
    { hiddenModels: [...(layout.hiddenModels ?? [])].sort() },
    snapshot.statisticsRevision,
    active,
  );
  const accounts = (query.data?.accounts ?? []).filter((account) =>
    agentShown(account.provider, layout, focusVendor),
  );
  const usd = (amount: number) => listPrice(amount, "USD", language);
  return (
    <section
      className="hud-source-plan-value"
      style={{
        display: "grid",
        gridTemplateColumns: "minmax(0, 1fr)",
        gap: 6,
        paddingTop: bare ? 0 : 8,
        boxShadow: bare ? undefined : `inset 0 1px 0 ${colors.divider}`,
      }}
    >
      <AddedSectionHeading title={t("Plan value")} colors={colors}>
        <span style={{ fontSize: 11, color: colors.tertiary }}>
          {t("30 days · API list prices")}
        </span>
      </AddedSectionHeading>
      {!query.data && (
        <SectionNote colors={colors}>
          {query.error ?? t("Loading plan value…")}
        </SectionNote>
      )}
      {query.data && !accounts.length && (
        <SectionNote colors={colors}>{t("No plans to value")}</SectionNote>
      )}
      {accounts.map((account) => {
        const value = account.value;
        const worth = value
          ? `${listCost(value.amounts, language) || "—"}${value.unpricedTokens > 0 ? "+" : ""}`
          : null;
        return (
          <div key={account.id} style={{ display: "grid", gap: 2 }}>
            <div
              style={{
                display: "flex",
                alignItems: "center",
                gap: 6,
                fontSize: 12,
                minWidth: 0,
              }}
            >
              {Logo && (
                <Logo source={account.provider} size={13} light={light} />
              )}
              <strong style={{ ...ellipsis, color: colors.text }}>
                {account.plan?.name ?? account.planLabel ?? "—"}
              </strong>
              <span
                style={{ ...ellipsis, fontSize: 11, color: colors.secondary }}
              >
                {account.name}
              </span>
              <span style={{ flex: 1 }} />
              <span
                title={t(
                  "The 30-day API value divided by the monthly list price",
                )}
                style={{ ...tabular, fontWeight: 600, color: colors.text }}
              >
                {ratioText(account, t)}
              </span>
            </div>
            <div
              style={{
                ...tabular,
                ...ellipsis,
                fontSize: 11,
                color: colors.secondary,
              }}
            >
              {account.plan
                ? t("{{price}}/mo", { price: usd(account.plan.monthlyUSD) })
                : t("No monthly list price")}
              {" · "}
              {worth
                ? t("API value {{worth}}", { worth })
                : account.unavailable}
            </div>
          </div>
        );
      })}
    </section>
  );
}
