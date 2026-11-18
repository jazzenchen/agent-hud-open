import { useLayoutEffect, useRef } from "react";
import { X } from "lucide-react";
import { useT } from "@agent-hud/ui/format-context";
import { INK, pinNumbers, type Mark } from "./mark-model";
import type { MarkColors } from "./mark-theme";

/** A comment's field, as tall as its words. */
function Words({
  text,
  write,
  done,
  colors,
}: {
  text: string;
  write: (text: string) => void;
  done: () => void;
  colors: MarkColors;
}) {
  const t = useT();
  const field = useRef<HTMLTextAreaElement>(null);
  useLayoutEffect(() => {
    const element = field.current;
    if (!element) return;
    element.style.height = "0px";
    element.style.height = `${element.scrollHeight}px`;
  }, [text]);
  return (
    <textarea
      ref={field}
      rows={1}
      value={text}
      placeholder={t("What about this spot")}
      onChange={(event) => write(event.target.value)}
      onBlur={done}
      style={{
        flex: 1,
        minWidth: 0,
        resize: "none",
        overflow: "hidden",
        padding: "5px 8px",
        border: `1px solid ${colors.line}`,
        borderRadius: 7,
        outline: 0,
        background: colors.field,
        color: colors.text,
        font: "inherit",
        fontSize: 13,
        lineHeight: 1.4,
        userSelect: "text",
      }}
    />
  );
}

/** The comments pinned on the picture, numbered as on it, each one to read
 * and rewrite. The pointer on one lights its pin; a comment left empty
 * takes its pin away. */
export function CommentList({
  marks,
  lit,
  light,
  write,
  drop,
  colors,
}: {
  marks: Mark[];
  /** The pin under the pointer, here or on the picture. */
  lit: number | null;
  light: (index: number | null) => void;
  write: (index: number, text: string) => void;
  drop: (index: number) => void;
  colors: MarkColors;
}) {
  const t = useT();
  const numbers = pinNumbers(marks);
  const pins = marks.flatMap((mark, index) =>
    mark.kind === "pin" ? [{ index, text: mark.text }] : [],
  );
  return (
    <aside
      style={{
        width: 264,
        flexShrink: 0,
        display: "flex",
        flexDirection: "column",
        background: colors.ground,
        borderLeft: `1px solid ${colors.line}`,
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "baseline",
          gap: 6,
          padding: "14px 16px 8px",
          fontSize: 13,
          fontWeight: 600,
        }}
      >
        {t("Comments")}
        {pins.length > 0 && (
          <span style={{ color: colors.quiet, fontWeight: 500 }}>
            {pins.length}
          </span>
        )}
      </div>
      <div
        style={{
          flex: 1,
          minHeight: 0,
          overflowY: "auto",
          padding: "0 10px 12px",
        }}
      >
        {pins.length === 0 ? (
          <p
            style={{
              margin: "4px 6px",
              color: colors.quiet,
              fontSize: 12.5,
              lineHeight: 1.45,
            }}
          >
            {t(
              "Choose Comment, then click the picture to pin a numbered note.",
            )}
          </p>
        ) : (
          pins.map(({ index, text }) => (
            <div
              key={index}
              onPointerEnter={() => light(index)}
              onPointerLeave={() => light(null)}
              style={{
                display: "flex",
                alignItems: "flex-start",
                gap: 8,
                padding: 6,
                borderRadius: 9,
                background: lit === index ? colors.group : "transparent",
              }}
            >
              <span
                style={{
                  display: "inline-grid",
                  placeItems: "center",
                  flexShrink: 0,
                  width: 20,
                  height: 20,
                  marginTop: 5,
                  borderRadius: "50%",
                  background: INK,
                  color: "#FFFFFF",
                  fontSize: 11,
                  fontWeight: 700,
                }}
              >
                {numbers.get(index)}
              </span>
              <Words
                text={text}
                write={(next) => write(index, next)}
                done={() => !text.trim() && drop(index)}
                colors={colors}
              />
              <button
                type="button"
                aria-label={t("Remove")}
                title={t("Remove")}
                onClick={() => drop(index)}
                style={{
                  display: "inline-grid",
                  placeItems: "center",
                  flexShrink: 0,
                  width: 22,
                  height: 22,
                  marginTop: 4,
                  border: 0,
                  borderRadius: 6,
                  background: "transparent",
                  color: colors.quiet,
                  visibility: lit === index ? "visible" : "hidden",
                  cursor: "default",
                }}
              >
                <X size={13} strokeWidth={2.2} />
              </button>
            </div>
          ))
        )}
      </div>
      <p
        style={{
          margin: 0,
          padding: "10px 16px 14px",
          borderTop: `1px solid ${colors.line}`,
          color: colors.quiet,
          fontSize: 11.5,
          lineHeight: 1.45,
        }}
      >
        {t("The comments go under the picture as a numbered list.")}
      </p>
    </aside>
  );
}
