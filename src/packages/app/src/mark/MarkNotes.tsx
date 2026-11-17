import { useEffect, useRef } from "react";
import { useT } from "@agent-hud/ui/format-context";
import { INK, pinNumbers, type Mark } from "./mark-model";
import type { MarkColors } from "./MarkWindow";

/** The comments beside the picture, one per pin, by its number; a new pin's
 * field takes the keys at once. */
export function MarkNotes({
  marks,
  write,
  colors,
}: {
  marks: Mark[];
  write: (index: number, text: string) => void;
  colors: MarkColors;
}) {
  const t = useT();
  const numbers = pinNumbers(marks);
  const last = useRef<HTMLInputElement>(null);
  const count = numbers.size;
  useEffect(() => {
    const frame = requestAnimationFrame(() => last.current?.focus());
    return () => cancelAnimationFrame(frame);
  }, [count]);
  return (
    <div
      style={{
        width: 260,
        flexShrink: 0,
        display: "flex",
        flexDirection: "column",
        gap: 8,
        padding: 14,
        borderLeft: `1px solid ${colors.line}`,
        overflowY: "auto",
        userSelect: "text",
      }}
    >
      <span style={{ fontSize: 12, fontWeight: 600, color: colors.quiet }}>
        {t("Comments")}
      </span>
      {count === 0 && (
        <span style={{ fontSize: 12, lineHeight: 1.5, color: colors.quiet }}>
          {t("Choose Comment, then click the picture to pin a numbered note.")}
        </span>
      )}
      {marks.map((mark, index) =>
        mark.kind !== "pin" ? null : (
          <label
            key={index}
            style={{ display: "flex", alignItems: "center", gap: 8 }}
          >
            <span
              style={{
                width: 20,
                height: 20,
                flexShrink: 0,
                borderRadius: 10,
                background: INK,
                color: "#FFFFFF",
                fontSize: 11,
                fontWeight: 600,
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
              }}
            >
              {numbers.get(index)}
            </span>
            <input
              ref={numbers.get(index) === count ? last : undefined}
              value={mark.text}
              placeholder={t("What about this spot")}
              onChange={(event) => write(index, event.target.value)}
              style={{
                flex: 1,
                minWidth: 0,
                height: 30,
                padding: "0 8px",
                border: `1px solid ${colors.line}`,
                borderRadius: 6,
                outline: 0,
                background: colors.field,
                color: colors.text,
                fontSize: 13,
                fontFamily: "inherit",
              }}
            />
          </label>
        ),
      )}
    </div>
  );
}
