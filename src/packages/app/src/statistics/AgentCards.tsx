import { useMemo, type Dispatch, type SetStateAction } from "react";
import { ChevronDown } from "lucide-react";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@agent-hud/ui/components/dropdown-menu";
import { HUDArtwork as HUDLogo } from "@agent-hud/hud/HUDArtwork";
import { vendorKey } from "@agent-hud/hud/model";
import { useT } from "@agent-hud/ui/format-context";
import type {
  BillingAccount,
  Quota,
  StatisticsResponse,
} from "@agent-hud/protocol";
import { storageKeys } from "../lib/storage-keys";
import { card, plain, row, type StatsTheme } from "./StatsTheme";
import { Bar } from "./dashboard/Placeholder";
import { Heading } from "./StatsParts";
import { ProviderCard } from "./ProviderCard";
import type { QuotaRow } from "./dashboard/QuotaRows";
import { QuotaCard, quotaOnly } from "./dashboard/QuotaCard";
import type { Pending } from "./dashboard/pending";

/** The Usage page's By agent section: a card per shown agent in a grid
 * of cards no wider than about 450 points, as many to a row as fit, with a
 * menu that hides and shows them. An agent with quota windows but no tokens
 * recorded here gets a card of its quota alone. */
export function AgentCards({
  data,
  hiddenAgentCards,
  setHiddenAgentCards,
  selectedProviders,
  selectedModels,
  label,
  theme,
  light,
  highlightedVendor,
  billing,
  quota,
  names,
  pending,
}: {
  data: StatisticsResponse | null;
  hiddenAgentCards: string[];
  setHiddenAgentCards: Dispatch<SetStateAction<string[]>>;
  selectedProviders: string[] | null;
  selectedModels: string[] | null;
  label: string;
  theme: StatsTheme;
  light: boolean;
  highlightedVendor: string | null;
  billing: BillingAccount[];
  /** Each quota window's days, for the cards of the agents they belong to. */
  quota?: { rows: QuotaRow[]; live: Map<string, Quota> };
  /** Agents' names, for the cards of quota alone. */
  names: Map<string, string>;
  /** The range's and the quota's answers, while a filter change waits. */
  pending: { range: Pending; quota: Pending };
}) {
  const t = useT();
  const visibleProviders = useMemo(
    () =>
      (data?.providers ?? []).filter(
        (provider) => !hiddenAgentCards.includes(provider.id),
      ),
    [data?.providers, hiddenAgentCards],
  );
  const toggleAgentCard = (providerId: string, shown: boolean) => {
    setHiddenAgentCards((previous) => {
      const next = shown
        ? previous.filter((id) => id !== providerId)
        : [...new Set([...previous, providerId])];
      localStorage.setItem(storageKeys.hiddenAgentCards, JSON.stringify(next));
      return next;
    });
  };
  return (
    <>
      <Heading
        theme={theme}
        trailing={
          data ? (
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <button
                  type="button"
                  style={{ ...plain, ...row, gap: 4, color: theme.secondary }}
                >
                  {t("{{agents}} shown", { agents: visibleProviders.length })}
                  <ChevronDown size={10} />
                </button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuCheckboxItem
                  checked={hiddenAgentCards.length === 0}
                  onSelect={(event) => event.preventDefault()}
                  onCheckedChange={() => {
                    setHiddenAgentCards([]);
                    localStorage.removeItem(storageKeys.hiddenAgentCards);
                  }}
                >
                  {t("Show all")}
                </DropdownMenuCheckboxItem>
                <DropdownMenuSeparator />
                {data.providers.map((provider) => (
                  <DropdownMenuCheckboxItem
                    key={provider.id}
                    checked={!hiddenAgentCards.includes(provider.id)}
                    onSelect={(event) => event.preventDefault()}
                    onCheckedChange={(checked) =>
                      toggleAgentCard(provider.id, Boolean(checked))
                    }
                  >
                    <HUDLogo source={provider.id} size={12} light={light} />
                    {provider.name}
                  </DropdownMenuCheckboxItem>
                ))}
              </DropdownMenuContent>
            </DropdownMenu>
          ) : undefined
        }
      >
        {t("By agent")}
      </Heading>
      {!data && (
        <div className="stats-agent-grid">
          {[0, 1, 2].map((index) => (
            <div
              key={index}
              style={{
                ...card(theme),
                display: "grid",
                gap: 12,
                minHeight: 300,
              }}
            >
              <Bar width="45%" height={14} theme={theme} />
              <Bar width="35%" height={26} theme={theme} />
              <Bar height={60} theme={theme} />
              <Bar height={8} theme={theme} />
              <Bar width="80%" height={10} theme={theme} />
              <Bar width="70%" height={10} theme={theme} />
              <Bar height={56} theme={theme} />
            </div>
          ))}
        </div>
      )}
      {data && (
        <div className="stats-agent-grid">
          {visibleProviders.map((provider) => (
            <ProviderCard
              key={provider.id}
              provider={provider}
              total={data.totals.selectedTokens}
              perRow={3}
              label={label}
              theme={theme}
              highlighted={highlightedVendor === vendorKey(provider.id)}
              billing={billing}
              pending={[pending.range, pending.quota]}
              quota={
                quota && {
                  rows: quota.rows.filter(
                    (entry) =>
                      vendorKey(entry.window.vendor) === vendorKey(provider.id),
                  ),
                  live: quota.live,
                }
              }
            />
          ))}
          {quota &&
            quotaOnly(quota.rows, visibleProviders).map(([vendor, rows]) => (
              <QuotaCard
                key={vendor}
                vendor={vendor}
                name={names.get(vendor) ?? vendor}
                rows={rows}
                live={quota.live}
                pending={pending.quota}
                theme={theme}
                light={light}
              />
            ))}
        </div>
      )}
      {data && !visibleProviders.length && (
        <p style={{ color: theme.secondary, fontSize: 12, margin: "4px 0" }}>
          {selectedProviders?.length === 0 ||
          selectedModels?.length === 0 ||
          (data.providers.length > 0 && visibleProviders.length === 0)
            ? t("No agents or models selected.")
            : t("No recorded agent usage in this range.")}
        </p>
      )}
    </>
  );
}
