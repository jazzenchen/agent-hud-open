export function windowAppearance(
  saved: string | undefined,
  initial: string | undefined,
): "light" | "dark" | "system" {
  const appearance = saved ?? initial;
  return appearance === "light" || appearance === "dark"
    ? appearance
    : "system";
}
