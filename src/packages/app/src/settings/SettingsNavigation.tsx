import { createContext, useContext } from "react";

/** Opens a Settings page by id, for a pane that points at another one (an
 * extension's page can name Agent plugins). Null outside Settings. */
export const SettingsNavigation = createContext<
  ((page: string) => void) | null
>(null);

export function useOpenSettingsPage() {
  return useContext(SettingsNavigation);
}
