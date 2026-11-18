import { useT } from "@agent-hud/ui/format-context";
import { pinNumbers, pinRadius, type Mark } from "./mark-model";
import type { MarkColors } from "./mark-theme";

/** Where a note sits beside its pin, `gap` clear of its centre: to the
 * right, or to the left near the picture's right edge. */
const beside = (x: number, gap: number, width: number) =>
  x / width > 0.7
    ? { right: `${100 - ((x - gap) / width) * 100}%` }
    : { left: `${((x + gap) / width) * 100}%` };

/** Each pin's note, on the picture beside its pin, on an opaque card so its
 * words read on any picture: the one being written as a field, the others
 * while their pin is lit. Pressing a pin opens its note again; a note left
 * empty takes its pin away. */
export function PinNotes({
  size,
  scale,
  marks,
  editing,
  edit,
  lit,
  light,
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
  lit: number | null;
  light: (index: number | null) => void;
  write: (index: number, text: string) => void;
  drop: (index: number) => void;
  colors: MarkColors;
}) {
  const t = useT();
  const numbers = pinNumbers(marks);
  const hit = Math.max(pinRadius(size.w) * scale * 2, 22);
  const finish = (index: number) => {
    const mark = marks[index];
    if (mark?.kind === "pin" && !mark.text.trim()) drop(index);
    edit(null);
  };
  const card = {
    position: "absolute",
    transform: "translateY(-50%)",
    zIndex: 2,
    border: `1px solid ${colors.cardLine}`,
    borderRadius: 9,
    background: colors.card,
    color: colors.text,
    boxShadow: "0 6px 24px rgba(0,0,0,0.28)",
    fontSize: 13,
  } as const;
  return (
    <>
      {marks.map((mark, index) => {
        if (mark.kind !== "pin") return null;
        const left = `${(mark.x / size.w) * 100}%`;
        const top = `${(mark.y / size.h) * 100}%`;
        const place = beside(
          mark.x,
          pinRadius(size.w) * 1.4 + 6 / scale,
          size.w,
        );
        return (
          <div key={index}>
            <button
              type="button"
              aria-label={`${numbers.get(index)}. ${mark.text}`}
              onPointerDown={(event) => event.stopPropagation()}
              onClick={() => edit(index)}
              onPointerEnter={() => light(index)}
              onPointerLeave={() => light(null)}
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
                  width: 260,
                  height: 34,
                  padding: "0 11px",
                  outline: 0,
                  fontFamily: "inherit",
                  userSelect: "text",
                }}
              />
            ) : lit === index && mark.text ? (
              <span
                style={{
                  ...card,
                  top,
                  ...place,
                  maxWidth: 300,
                  padding: "7px 11px",
                  lineHeight: 1.4,
                  pointerEvents: "none",
                  whiteSpace: "pre-wrap",
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
