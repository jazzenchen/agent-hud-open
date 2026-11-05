import type {
  PanelFilterLayout,
  PanelLayout,
  PanelSectionId,
  QuotaHistoryWindow,
} from "@agent-hud/protocol";
import { agentArtworkKey } from "../agent-artwork.ts";

/** The sections a panel shows only once its layout names them in `sections`:
 * a layout saved before them is drawn as it was (rules `ADDED_SECTIONS`). */
export const addedSections = ["quotaHistory", "planValue"] as const;
export type AddedSection = (typeof addedSections)[number];

export const isAddedSection = (key: string): key is AddedSection =>
  (addedSections as readonly string[]).includes(key);

/** Whether the layout shows an added section: named and not hidden. */
export function addedSectionShown(
  layout: PanelLayout,
  key: AddedSection,
): boolean {
  return (
    (layout.sections ?? []).includes(key) &&
    !(layout.hiddenSections ?? []).includes(key)
  );
}

/** The layout with an added section shown or hidden and every section named
 * in `order`, each other added section keeping whether it shows. */
export function withAddedSection(
  layout: PanelLayout,
  order: readonly string[],
  key: AddedSection,
  shown: boolean,
): PanelLayout {
  const hidden = new Set<PanelSectionId>(layout.hiddenSections ?? []);
  for (const section of addedSections)
    if (!addedSectionShown(layout, section)) hidden.add(section);
  if (shown) hidden.delete(key);
  else hidden.add(key);
  return {
    ...layout,
    sections: [...order] as PanelSectionId[],
    hiddenSections: [...hidden],
  };
}

/** Whether the quota history or the plan value shows an agent's part: the
 * agent whose own panel this is, else the layout's agents, else every one. */
export function agentShown(
  agent: string,
  layout: PanelFilterLayout,
  focusVendor?: string,
): boolean {
  if (focusVendor)
    return agentArtworkKey(agent) === agentArtworkKey(focusVendor);
  return !layout.agents?.length || layout.agents.includes(agent);
}

/** Whether the quota history shows a window: its agent is shown and, when
 * calls spent it in the stretch, one of their models is not hidden. */
export function historyWindowShown(
  window: Pick<QuotaHistoryWindow, "vendor" | "models">,
  layout: PanelFilterLayout,
  focusVendor?: string,
): boolean {
  const hidden = layout.hiddenModels ?? [];
  return (
    agentShown(window.vendor, layout, focusVendor) &&
    (!window.models?.length ||
      window.models.some((model) => !hidden.includes(model)))
  );
}
