// Path geometry for the Usage page's charts: smooth lines through points
// that never overshoot them, and donut slices.

type Point = [number, number];

/** An SVG path through `points` as a monotone cubic curve (Fritsch–Carlson),
 * so a smoothed line never rises above a peak or dips below zero between
 * points. Starts with M unless `continued`, then it starts with L. */
export function smoothPath(points: Point[], continued = false): string {
  if (!points.length) return "";
  const start = `${continued ? "L" : "M"}${points[0][0]},${points[0][1]}`;
  if (points.length === 1) return start;
  const n = points.length;
  const secants: number[] = [];
  for (let i = 0; i < n - 1; i++) {
    const dx = points[i + 1][0] - points[i][0];
    secants.push(dx === 0 ? 0 : (points[i + 1][1] - points[i][1]) / dx);
  }
  const tangents = points.map((_, i) => {
    if (i === 0) return secants[0];
    if (i === n - 1) return secants[n - 2];
    const a = secants[i - 1];
    const b = secants[i];
    return a * b <= 0 ? 0 : (2 * a * b) / (a + b);
  });
  let path = start;
  for (let i = 0; i < n - 1; i++) {
    const [x0, y0] = points[i];
    const [x1, y1] = points[i + 1];
    const third = (x1 - x0) / 3;
    path += ` C${x0 + third},${y0 + tangents[i] * third} ${x1 - third},${y1 - tangents[i + 1] * third} ${x1},${y1}`;
  }
  return path;
}

/** A donut slice from `start` to `end` (radians, clockwise from 12 o'clock). */
export function arcPath(
  cx: number,
  cy: number,
  outer: number,
  inner: number,
  start: number,
  end: number,
): string {
  const sweep = Math.min(end - start, Math.PI * 2 - 1e-6);
  const stop = start + sweep;
  const at = (radius: number, angle: number) =>
    `${cx + radius * Math.sin(angle)},${cy - radius * Math.cos(angle)}`;
  const large = sweep > Math.PI ? 1 : 0;
  return [
    `M${at(outer, start)}`,
    `A${outer},${outer} 0 ${large} 1 ${at(outer, stop)}`,
    `L${at(inner, stop)}`,
    `A${inner},${inner} 0 ${large} 0 ${at(inner, start)}`,
    "Z",
  ].join(" ");
}
