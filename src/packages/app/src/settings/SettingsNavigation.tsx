import { createContext, useContext } from "react";

/** Opens a Settings page by id, for a pane that points at another one; an
 * agent's own page is "agents/" and its client id ("agents/claude"). Null
 * outside Settings. */
export const SettingsNavigation = createContext<
  ((page: string) => void) | null
>(null);

export function useOpenSettingsPage() {
  return useContext(SettingsNavigation);
}
