import {
  useEffect,
  useRef,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { vendorKey } from "@agent-hud/hud/model";
import { readJSON, storageKeys } from "../lib/storage-keys";

/** The vendor another window asked the Tokens page to show, for 2.5 s. Asking
 * clears the agent filter, shows that vendor's hidden cards, and once the
 * page has data scrolls its card into view. */
export function useProviderHighlight(
  active: boolean,
  loaded: boolean,
  setProviders: Dispatch<SetStateAction<string[] | null>>,
  setHiddenAgentCards: Dispatch<SetStateAction<string[]>>,
) {
  const [highlightedVendor, setHighlightedVendor] = useState<string | null>(
    null,
  );
  const highlightTimerRef = useRef<number | null>(null);
  const applyProviderHighlight = (providerName: string) => {
    const key = vendorKey(providerName);
    if (!key) return;
    setHighlightedVendor(key);
    setProviders(null);
    setHiddenAgentCards((previous) => {
      const next = previous.filter((id) => vendorKey(id) !== key);
      if (next.length !== previous.length)
        localStorage.setItem(
          storageKeys.hiddenAgentCards,
          JSON.stringify(next),
        );
      return next;
    });
    if (highlightTimerRef.current !== null) {
      window.clearTimeout(highlightTimerRef.current);
    }
    highlightTimerRef.current = window.setTimeout(() => {
      setHighlightedVendor(null);
      highlightTimerRef.current = null;
    }, 2500);
  };
  useEffect(() => {
    if (!active) return;
    const consumePending = () => {
      const pending = readJSON(storageKeys.highlightProvider) as {
        provider?: string;
        at?: number;
      } | null;
      if (!pending) return;
      localStorage.removeItem(storageKeys.highlightProvider);
      if (pending.provider && (!pending.at || Date.now() - pending.at < 15_000))
        applyProviderHighlight(pending.provider);
    };
    consumePending();
    window.addEventListener("focus", consumePending);
    window.addEventListener("storage", consumePending);
    const sub = isTauri()
      ? listen<{ provider: string; at: number }>(
          "statistics:provider",
          (event) => {
            localStorage.removeItem(storageKeys.highlightProvider);
            if (event.payload?.provider) {
              applyProviderHighlight(event.payload.provider);
            }
          },
        )
      : null;
    return () => {
      window.removeEventListener("focus", consumePending);
      window.removeEventListener("storage", consumePending);
      if (sub) void sub.then((unlisten) => unlisten());
    };
  }, [active]);
  useEffect(() => {
    if (!highlightedVendor || !loaded) return;
    const frame = window.requestAnimationFrame(() => {
      const target =
        document.getElementById(`agent-${highlightedVendor}`) ??
        document.getElementById(`api-${highlightedVendor}`);
      target?.scrollIntoView({ behavior: "smooth", block: "center" });
    });
    return () => window.cancelAnimationFrame(frame);
  }, [highlightedVendor, loaded]);
  return highlightedVendor;
}
