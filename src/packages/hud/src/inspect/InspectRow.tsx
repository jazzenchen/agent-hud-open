import { useState } from "react";
import { Plus, X } from "lucide-react";
import type { ClipKind, InspectContent } from "@agent-hud/protocol";
import type { Translate } from "@agent-hud/ui/format";
import { useT } from "@agent-hud/ui/format-context";
import { cardColumn } from "../card-column";
import { useGround } from "../ground";
import { alertInk, alertText } from "../tokens";

/** One thing in the window: a copy in the draft, or the latest copy not
 * added yet (`pending`). Both draw alike, so the field a comment is typed
 * into stays as the copy joins the draft. */
export interface ClipRow {
  id: string;
  kind: ClipKind;
  app: string | null;
  comment: string;
  /** The comments pinned on a marked picture. */
  marks?: string[];
  content: InspectContent | null;
  pending: boolean;
}

const fileName = (path: string) =>
  path.split(/[\\/]/).filter(Boolean).pop() ?? path;

function source(row: ClipRow, t: Translate) {
  const page = row.content?.kind === "page" ? row.content : null;
  if (page)
    return page.area
      ? t("Area of {{host}}", { host: page.host })
      : t("Element on {{host}}", { host: page.host });
  const what =
    row.kind === "image"
      ? t("Picture")
      : row.kind === "files"
        ? t("Files")
        : t("Text");
  return row.app ? t("{{what}} from {{app}}", { what, app: row.app }) : what;
}

function Preview({
  content,
  marks,
  mark,
  light,
}: {
  content: InspectContent | null;
  marks?: string[];
  /** Opens the picture in the markup window. */
  mark?: () => void;
  light: boolean;
}) {
  const t = useT();
  const ground = useGround(light);
  const quiet = { fontSize: 11, color: alertInk(0.5, ground) };
  if (!content) return <span style={quiet}>{t("Reading…")}</span>;
  const picture = (thumbnail: string) => (
    <span style={cardColumn(4)}>
      <button
        type="button"
        onClick={mark}
        title={t("Mark up the picture")}
        aria-label={t("Mark up the picture")}
        style={{
          justifySelf: "start",
          padding: 0,
          border: 0,
          background: "transparent",
          cursor: "zoom-in",
        }}
      >
        <img
          src={thumbnail}
          alt=""
          style={{
            display: "block",
            maxWidth: 180,
            maxHeight: 90,
            borderRadius: 6,
            boxShadow: `0 0 0 1px ${alertInk(0.12, ground)}`,
          }}
        />
      </button>
      {marks?.map((text, index) => (
        <span key={index} style={{ ...quiet, overflowWrap: "anywhere" }}>
          {index + 1}. {text}
        </span>
      ))}
    </span>
  );
  if (content.kind === "image") {
    return content.thumbnail ? (
      picture(content.thumbnail)
    ) : (
      <span style={quiet}>
        {content.width} × {content.height}
      </span>
    );
  }
  if (content.kind === "page") {
    const line = {
      overflow: "hidden",
      textOverflow: "ellipsis",
      whiteSpace: "nowrap",
    } as const;
    return (
      <span style={cardColumn(4)}>
        <span style={{ ...line, fontSize: 11.5, fontWeight: 500 }}>
          {content.title}
        </span>
        {content.selector && (
          <code
            style={{
              ...line,
              fontSize: 10.5,
              fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
              color: alertInk(0.6, ground),
            }}
          >
            {content.selector}
          </code>
        )}
        {content.thumbnail && picture(content.thumbnail)}
      </span>
    );
  }
  if (content.kind === "files") {
    const shown = content.paths.slice(0, 3).map(fileName).join(", ");
    const more = content.paths.length - 3;
    return (
      <span style={{ fontSize: 11.5, overflowWrap: "anywhere" }}>
        {shown}
        {more > 0 && ` +${more}`}
      </span>
    );
  }
  // The window shows the copy whole, scrolling; past what it carries it
  // says that all of it is sent.
  const cut = content.length > content.text.length;
  return (
    <span style={cardColumn(3)}>
      <span
        style={{
          fontSize: 11,
          lineHeight: "15px",
          maxHeight: 112,
          overflowY: "auto",
          whiteSpace: "pre-wrap",
          overflowWrap: "anywhere",
          fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
          color: alertInk(0.8, ground),
        }}
      >
        {content.text}
      </span>
      {cut && (
        <span style={quiet}>
          {t("All {{number}} characters are sent", { number: content.length })}
        </span>
      )}
    </span>
  );
}

export function InspectRow({
  row,
  number,
  mark,
  add,
  edit,
  remove,
  submit,
  light,
}: {
  row: ClipRow;
  number?: number;
  mark?: () => void;
  add: () => void;
  edit: (text: string) => void;
  remove: () => void;
  /** Return in the comment sends the message. */
  submit: () => void;
  light: boolean;
}) {
  const t = useT();
  const ground = useGround(light);
  const [comment, setComment] = useState(row.comment);
  const button = {
    display: "inline-flex",
    alignItems: "center",
    gap: 3,
    flexShrink: 0,
    height: 22,
    padding: "0 6px",
    border: 0,
    borderRadius: 6,
    background: "transparent",
    cursor: "pointer",
    fontSize: 11,
  } as const;
  return (
    <div
      style={{
        ...cardColumn(6),
        padding: "8px 8px 6px",
        borderRadius: 10,
        background: light ? "rgba(0,0,0,0.035)" : "rgba(255,255,255,0.05)",
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
        {number !== undefined && (
          <span
            style={{
              fontSize: 10.5,
              fontWeight: 600,
              fontVariantNumeric: "tabular-nums",
              color: alertInk(0.4, ground),
            }}
          >
            {number}
          </span>
        )}
        <span
          style={{
            flex: 1,
            minWidth: 0,
            fontSize: 10.5,
            color: alertInk(0.5, ground),
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}
        >
          {source(row, t)}
        </span>
        {row.pending && (
          <button
            type="button"
            onClick={add}
            style={{ ...button, color: alertText("turn", ground) }}
          >
            <Plus size={11} strokeWidth={2.4} />
            {t("Add")}
          </button>
        )}
        <button
          type="button"
          aria-label={t("Remove")}
          onClick={remove}
          style={{ ...button, color: alertInk(0.4, ground) }}
        >
          <X size={12} strokeWidth={2} />
        </button>
      </div>
      <Preview
        content={row.content}
        marks={row.marks}
        mark={mark}
        light={light}
      />
      <input
        type="text"
        value={comment}
        placeholder={t("Add a comment")}
        aria-label={t("Add a comment")}
        onChange={(event) => {
          setComment(event.target.value);
          edit(event.target.value);
        }}
        onKeyDown={(event) => {
          // Return while an input method is composing belongs to it.
          if (
            event.key === "Enter" &&
            !event.nativeEvent.isComposing &&
            event.keyCode !== 229
          ) {
            event.preventDefault();
            submit();
          }
        }}
        style={{
          border: 0,
          outline: 0,
          padding: "4px 0 2px",
          borderTop: `1px solid ${alertInk(0.08, ground)}`,
          background: "transparent",
          color: alertInk(0.92, ground),
          fontSize: 12,
          fontFamily: "inherit",
        }}
      />
    </div>
  );
}
