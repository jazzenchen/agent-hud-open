import { useLayoutEffect, type ReactNode } from "react";

/** Inside Suspense: the document's loading shell stays visible until the
 * actual page commits, including while its language catalog is loading. */
export function StartupReady({ children }: { children: ReactNode }) {
  useLayoutEffect(() => {
    const shell = document.getElementById("startup-shell");
    if (!shell) return;
    shell.hidden = true;
    // React cleans up layout effects when it hides a suspended tree, and
    // runs them again when it reveals it. Keep the same shell throughout.
    return () => {
      shell.hidden = false;
    };
  }, []);
  return children;
}
