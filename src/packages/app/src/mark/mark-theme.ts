// The markup window's colours in light and dark, and the size the picture
// shows at.
import { useLayoutEffect, useState, type RefObject } from "react";

export interface MarkColors {
  /** The toolbar and the comment list. */
  ground: string;
  /** Behind the picture. */
  stage: string;
  text: string;
  quiet: string;
  line: string;
  /** The tool group, and the chosen tool in it. */
  group: string;
  chosen: string;
  chosenShadow: string;
  /** A comment's card on the picture: opaque, so its words read on any
   * picture. */
  card: string;
  cardLine: string;
  /** A comment's field in the list. */
  field: string;
}

export const colorsFor = (light: boolean): MarkColors =>
  light
    ? {
        ground: "#F6F6F8",
        stage: "#E6E6EB",
        text: "#1C1C1E",
        quiet: "rgba(60,60,67,0.62)",
        line: "rgba(0,0,0,0.09)",
        group: "rgba(0,0,0,0.05)",
        chosen: "#FFFFFF",
        chosenShadow: "0 1px 2px rgba(0,0,0,0.16)",
        card: "#FFFFFF",
        cardLine: "rgba(0,0,0,0.1)",
        field: "#FFFFFF",
      }
    : {
        ground: "#242426",
        stage: "#141416",
        text: "#F2F2F7",
        quiet: "rgba(235,235,245,0.6)",
        line: "rgba(255,255,255,0.08)",
        group: "rgba(255,255,255,0.06)",
        chosen: "rgba(255,255,255,0.16)",
        chosenShadow: "none",
        card: "#2C2C2E",
        cardLine: "rgba(255,255,255,0.12)",
        field: "rgba(255,255,255,0.06)",
      };

/** Room kept around the picture on its stage. */
const MARGIN = 28;

/** The size the picture shows at: as large as fits `stage`, never past one
 * point per pixel, so a small picture stays sharp. */
export function useFit(
  stage: RefObject<HTMLDivElement | null>,
  size: { w: number; h: number } | null,
) {
  const [fit, setFit] = useState({ w: 0, h: 0 });
  useLayoutEffect(() => {
    const element = stage.current;
    if (!element || !size) return;
    const measure = () => {
      const room = element.getBoundingClientRect();
      const scale = Math.min(
        (room.width - MARGIN * 2) / size.w,
        (room.height - MARGIN * 2) / size.h,
        1,
      );
      setFit({ w: size.w * scale, h: size.h * scale });
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, [size?.w, size?.h]);
  return fit;
}
