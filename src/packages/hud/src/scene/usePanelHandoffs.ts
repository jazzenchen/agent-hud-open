import { useCallback, useMemo } from "react";
import type { ListedSessions, StatsTarget } from "../panel/listed-sessions";

/** The panel's ways to another window: a card's Settings page, the session
 * rows' Statistics, and the footer's buttons. Each ends an agent's edit hold
 * first. */
export function usePanelHandoffs({
  endHold,
  handoff,
  openSettingsPage,
  onStats,
  configurePanel,
  showPanelStats,
  onUpdate,
}: {
  endHold: () => void;
  handoff: (callback?: () => void) => void;
  openSettingsPage?: (page: string) => void;
  onStats?: (target?: StatsTarget) => void;
  configurePanel?: () => void;
  showPanelStats?: () => void;
  onUpdate?: () => void;
}) {
  // A card's Open Agent plugins hands over as the settings button does.
  const openSettings = useMemo(
    () =>
      openSettingsPage
        ? (page: string) => {
            endHold();
            handoff(() => openSettingsPage(page));
          }
        : undefined,
    [openSettingsPage, endHold, handoff],
  );
  // The panel's session rows hand over as Open's ScreenHUD does: the HUD
  // collapses first, then the Statistics window opens.
  const openStatsAt = useCallback(
    (target: StatsTarget) => {
      if (!onStats) return;
      endHold();
      handoff(() => onStats(target));
    },
    [onStats, endHold, handoff],
  );
  const listedSessions = useMemo<ListedSessions>(
    () => ({ openStats: openStatsAt }),
    [openStatsAt],
  );
  // The footer's Settings, Statistics and update arrow end an agent's edit
  // hold, then hand over: each opens another window.
  const footer = useMemo(
    () => ({
      configure:
        configurePanel &&
        (() => {
          endHold();
          configurePanel();
        }),
      stats:
        showPanelStats &&
        (() => {
          endHold();
          showPanelStats();
        }),
      update:
        onUpdate &&
        (() => {
          endHold();
          handoff(onUpdate);
        }),
    }),
    [configurePanel, showPanelStats, onUpdate, endHold, handoff],
  );
  return { openSettings, listedSessions, footer };
}
