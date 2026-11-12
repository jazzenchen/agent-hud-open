// Where a control sits in the row, column or grid that holds it.
import type { CSSProperties } from "react";
import type { Node } from "./drawing";

/** Where a control sits in its parent: a width of its own, or in a row an equal
 * share of the room left for a column, a grid or a section. */
export function placing(width: unknown, shares: boolean): CSSProperties | null {
  if (typeof width === "number")
    return { flex: `0 0 ${width}px`, width, minWidth: 0, maxWidth: "100%" };
  return shares ? { flex: "1 1 0", minWidth: 0 } : null;
}

/** The cells a control covers in its grid. */
export function spanning(node: Node, parent: string): CSSProperties | null {
  if (parent !== "grid" || !(node.colSpan || node.rowSpan)) return null;
  const span = (value: unknown) =>
    typeof value === "number" && value > 1 ? `span ${value}` : undefined;
  return { gridColumn: span(node.colSpan), gridRow: span(node.rowSpan) };
}
