/** In-dock alert that stays within the native HUD's existing bounds. */
export function DockQuotaBadge({
  description,
  scale,
}: {
  description: string;
  scale: number;
}) {
  return (
    <span
      role="status"
      aria-label={description}
      style={{
        position: "absolute",
        left: 0,
        bottom: 0,
        minWidth: 12 * scale,
        height: 12 * scale,
        borderRadius: "50%",
        background: "#b42318",
        color: "#fff",
        fontSize: 10 * scale,
        fontWeight: 700,
        lineHeight: `${12 * scale}px`,
        textAlign: "center",
        pointerEvents: "none",
      }}
    >
      !
    </span>
  );
}
