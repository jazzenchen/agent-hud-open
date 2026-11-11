export function dockNumberColors(
  style: string,
  transparency: number,
  light: boolean,
  color: string,
) {
  if (style === "grayscale") {
    return {
      color: light ? "#262626" : "#ededed",
      opacity: Number.isFinite(transparency)
        ? 1 - Math.max(0, Math.min(0.9, transparency))
        : 1,
    };
  }
  return { color };
}
