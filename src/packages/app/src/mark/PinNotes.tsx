import { useState } from "react";
import { useT } from "@agent-hud/ui/format-context";
import { pinNumbers, pinRadius, type Mark } from "./mark-model";
import type { MarkColors } from "./MarkWindow";

/** Where a note sits beside its pin: to the right, or to the left near the
 * picture's right edge. */
const beside = (x: number, width: number) =>
  x / width > 0.7
    ? { right: `${100 - (x / width) * 100}%`, marginRight: 4 }
    : { left: `${(x / width) * 100}%`, marginLeft: 4 };

/** Each pin's note, on the picture beside its pin: the one being written as a
 * small field, the others only while the pointer is on their pin. Pressing
 * a pin opens its note again; a note left empty takes its pin away. */
export function PinNotes({
  size,
  scale,
  marks,
  editing,
  edit,
  write,
  drop,
  colors,
}: {
  size: { w: number; h: number };
  /** Shown pixels per picture pixel. */
  scale: number;
  marks: Mark[];
  editing: number | null;
  edit: (index: number | null) => void;
  write: (index: number, text: string) => void;
  drop: (index: number) => void;
  colors: MarkColors;
}) {
  const t = useT();
  const [hover, setHover] = useState<number | null>(null);
  const numbers = pinNumbers(marks);
  const hit = pinRadius(size.w) * scale * 2;
  const finish = (index: number) => {
    const mark = marks[index];
    if (mark?.kind === "pin" && !mark.text.trim()) drop(index);
    edit(null);
  };
  const card = {
    position: "absolute",
    transform: "translateY(-50%)",
    zIndex: 2,
    borderRadius: 8,
    background: colors.field,
    color: colors.text,
    boxShadow: "0 4px 18px rgba(0,0,0,0.22)",
    fontSize: 13,
  } as const;
  return (
    <>
      {marks.map((mark, index) => {
        if (mark.kind !== "pin") return null;
        const left = `${(mark.x / size.w) * 100}%`;
        const top = `${(mark.y / size.h) * 100}%`;
        const place = beside(mark.x + pinRadius(size.w) * 1.3, size.w);
        return (
          <div key={index}>
            <button
              type="button"
              aria-label={`${numbers.get(index)}. ${mark.text}`}
              onPointerDown={(event) => event.stopPropagation()}
              onClick={() => edit(index)}
              onPointerEnter={() => setHover(index)}
              onPointerLeave={() => setHover(null)}
              style={{
                position: "absolute",
                left,
                top,
                width: hit,
                height: hit,
                transform: "translate(-50%, -50%)",
                border: 0,
                borderRadius: "50%",
                background: "transparent",
                cursor: "pointer",
              }}
            />
            {editing === index ? (
              <input
                autoFocus
                value={mark.text}
                placeholder={t("What about this spot")}
                onChange={(event) => write(index, event.target.value)}
                onBlur={() => finish(index)}
                onKeyDown={(event) => {
                  if (event.nativeEvent.isComposing) return;
                  if (event.key === "Enter" || event.key === "Escape") {
                    event.preventDefault();
                    event.stopPropagation();
                    finish(index);
                  }
                }}
                onPointerDown={(event) => event.stopPropagation()}
                style={{
                  ...card,
                  top,
                  ...place,
                  width: 240,
                  height: 32,
                  padding: "0 10px",
                  border: 0,
                  outline: 0,
                  fontFamily: "inherit",
                }}
              />
            ) : hover === index && mark.text ? (
              <span
                style={{
                  ...card,
                  top,
                  ...place,
                  maxWidth: 280,
                  padding: "6px 10px",
                  lineHeight: 1.4,
                  pointerEvents: "none",
                }}
              >
                {mark.text}
              </span>
            ) : null}
          </div>
        );
      })}
    </>
  );
}
