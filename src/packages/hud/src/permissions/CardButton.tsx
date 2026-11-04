import type { ButtonHTMLAttributes } from "react";

/** The request and alert cards' pill button: an icon and a label, drawn with
 * an outline (`edge`), a fill, or both. */
export function CardButton({
  edge,
  fill,
  color,
  style,
  ...button
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  edge?: string;
  fill?: string;
  color: string;
}) {
  return (
    <button
      type="button"
      {...button}
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 4,
        height: 22,
        padding: "0 9px",
        borderRadius: 6,
        border: edge ? `1px solid ${edge}` : 0,
        background: fill ?? "transparent",
        color,
        fontSize: 11,
        fontWeight: 500,
        cursor: button.disabled ? "default" : "pointer",
        ...style,
      }}
    />
  );
}
