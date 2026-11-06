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

function agentRows<T>(values: T[]): T[][] {
  const sizes = Array.from({ length: Math.floor(values.length / 3) }, () => 3);
  if (values.length % 3 === 2) sizes.push(2);
  if (values.length % 3 === 1) {
    if (!sizes.length) sizes.push(1);
    else {
      sizes.pop();
      sizes.push(2, 2);
    }
  }
  let offset = 0;
  return sizes.map((size) => {
    const result = values.slice(offset, offset + size);
    offset += size;
    return result;
  });
}

/** The Tokens page's By agent section: a card per shown agent, in rows,
 * with a menu that hides and shows them. */
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
}) {
  const zh = Boolean(theme.zh);
  const t = (en: string, cn: string) => (zh ? cn : en);
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
                  {t(
                    `${visibleProviders.length} shown`,
                    `显示 ${visibleProviders.length} 个`,
                  )}
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
                  {t("Show all", "全部显示")}
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
        {t("By agent", "按智能体")}
      </Heading>
      {!data && (
        <div
          className="statistics-agent-row"
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(3,minmax(0,1fr))",
            gap: 12,
          }}
        >
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
      {data &&
        agentRows(visibleProviders).map((providers, index) => (
          <div
            key={index}
            className="statistics-agent-row"
            style={{
              display: "grid",
              gridTemplateColumns: `repeat(${providers.length},minmax(0,1fr))`,
              gap: 12,
            }}
          >
            {providers.map((provider) => (
              <ProviderCard
                key={provider.id}
                provider={provider}
                total={data.totals.selectedTokens}
                perRow={providers.length}
                label={label}
                theme={theme}
                highlighted={highlightedVendor === vendorKey(provider.id)}
                billing={billing}
                quota={
                  quota && {
                    rows: quota.rows.filter(
                      (entry) =>
                        vendorKey(entry.window.vendor) ===
                        vendorKey(provider.id),
                    ),
                    live: quota.live,
                  }
                }
              />
            ))}
          </div>
        ))}
      {data && !visibleProviders.length && (
        <p style={{ color: theme.secondary, fontSize: 12, margin: "4px 0" }}>
          {selectedProviders?.length === 0 ||
          selectedModels?.length === 0 ||
          (data.providers.length > 0 && visibleProviders.length === 0)
            ? t("No agents or models selected.", "未选择智能体或模型。")
            : t(
                "No recorded agent usage in this range.",
                "该范围内无已记录的智能体用量。",
              )}
        </p>
      )}
    </>
  );
}
