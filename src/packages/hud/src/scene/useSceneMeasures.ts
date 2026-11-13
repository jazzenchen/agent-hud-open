import { useEffect, useLayoutEffect, useState, type RefObject } from "react";

/** The scene's width as laid out, at least 360. */
export function useSceneWidth(scene: RefObject<HTMLElement | null>): number {
  const [width, setWidth] = useState(720);
  useLayoutEffect(() => {
    if (!scene.current) return;
    setWidth(Math.max(360, scene.current.getBoundingClientRect().width));
    const observer = new ResizeObserver((entries) =>
      setWidth(Math.max(360, entries[0].contentRect.width)),
    );
    observer.observe(scene.current);
    return () => observer.disconnect();
  }, []);
  return width;
}

/** Whether the system appearance is light. */
export function useSystemLight(): boolean {
  const [light, setLight] = useState(false);
  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: light)");
    const change = () => setLight(media.matches);
    change();
    media.addEventListener("change", change);
    return () => media.removeEventListener("change", change);
  }, []);
  return light;
}
