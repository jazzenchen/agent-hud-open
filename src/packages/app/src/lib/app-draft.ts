import type { Snapshot } from "@agent-hud/protocol";

/** The snapshot as a HUD draft previews it: the app-wide settings the draft
 * changes (appearance, matching the screen, the menu bar icon) in place of
 * the saved ones. */
export function withAppDraft(
  snapshot: Snapshot | null,
  appDraft: unknown,
): Snapshot | null {
  if (!snapshot) return null;
  const draft = appDraft as Record<string, any> | null | undefined;
  const settings = {
    ...snapshot.settings,
    ...(draft?.appearance !== undefined
      ? { appearance: draft.appearance }
      : {}),
    ...(draft?.matchScreen !== undefined
      ? { appearanceFollowsScreen: draft.matchScreen }
      : {}),
    ...(draft?.showMenuBarIcon !== undefined
      ? { showMenuBarIcon: draft.showMenuBarIcon }
      : {}),
  };
  return { ...snapshot, settings };
}
