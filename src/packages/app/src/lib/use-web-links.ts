// A web link in any window (a docs link, a product page's order or store
// link) opens in the system's browser. The app's windows open no pages of
// their own, so without this a `target="_blank"` link does nothing.
import { useEffect, useRef } from "react";
import { isTauri } from "@tauri-apps/api/core";

export function useWebLinks(open: (url: string) => Promise<unknown>) {
  const latest = useRef(open);
  latest.current = open;
  useEffect(() => {
    if (!isTauri()) return;
    const click = (event: MouseEvent) => {
      if (event.defaultPrevented || event.button !== 0) return;
      const target = event.target as Element | null;
      const url = target?.closest?.("a[href]")?.getAttribute("href") ?? "";
      if (!url.startsWith("https://")) return;
      event.preventDefault();
      void latest
        .current(url)
        .catch((error) => console.warn("The link could not open", error));
    };
    document.addEventListener("click", click);
    return () => document.removeEventListener("click", click);
  }, []);
}
