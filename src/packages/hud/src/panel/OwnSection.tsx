import type { Snapshot } from "@agent-hud/protocol";
import type { HUDConfig } from "../model";
import { SourceSessions } from "./SessionsSection";
import { SourceTokenChart } from "./TokenChartSection";
import { QuotaHistorySection } from "./QuotaHistorySection";
import { PlanValueSection } from "./PlanValueSection";
import { addedSectionShown, isAddedSection } from "./added-sections";
import type { Logo } from "./theme";

/** Whether the panel shows one of its own sections other than the blocks. */
export function ownSectionShown(key: string, config: HUDConfig): boolean {
  if (key === "tokens") return config.tokenUsage;
  if (key === "sessions") return config.sessionList;
  return isAddedSection(key) && addedSectionShown(config.panelLayout, key);
}

/** The token chart, the sessions, the quota history or the plan value, as
 * the panel's layout sets them. */
export function OwnSection({
  id,
  config,
  snapshot,
  focusVendor,
  light,
  Logo,
  onStats,
  active,
  bare,
}: {
  id: string;
  config: HUDConfig;
  snapshot: Snapshot;
  focusVendor?: string;
  light: boolean;
  Logo: Logo;
  onStats?: () => void;
  active: boolean;
  bare: boolean;
}) {
  const shared = { snapshot, focusVendor, light, Logo, active, bare };
  const layout = config.panelLayout;
  if (id === "tokens")
    return <SourceTokenChart {...shared} layout={layout.tokens} />;
  if (id === "sessions")
    return (
      <SourceSessions {...shared} onStats={onStats} layout={layout.sessions} />
    );
  if (id === "quotaHistory")
    return <QuotaHistorySection {...shared} layout={layout.quotaHistory} />;
  if (id === "planValue")
    return <PlanValueSection {...shared} layout={layout.planValue} />;
  return null;
}
