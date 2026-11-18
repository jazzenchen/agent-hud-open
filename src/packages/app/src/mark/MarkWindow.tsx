import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useT } from "@agent-hud/ui/format-context";
import { CommentList } from "./CommentList";
import { MarkLayer } from "./MarkLayer";
import { MarkToolbar, TOOL_KEYS } from "./MarkToolbar";
import { PinNotes } from "./PinNotes";
import { markedPicture, type Mark, type MarkTool } from "./mark-model";
import { colorsFor, useFit } from "./mark-theme";

const typingIn = (target: EventTarget | null) =>
  target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement;

/** The markup window: a picture from the inspector, large, with a box, a
 * circle, a pen and numbered comments; the comments' list folds out at the
 * right, folded away at first. Done sends the picture with its marks drawn
 * in, and the comments, back to the inspector's draft. Keys: B, O, P and C
 * pick a tool, ⌘Z undoes, ⌘↩ is Done, Esc cancels. */
export default function MarkWindow({ theme }: { theme: "dark" | "light" }) {
  const t = useT();
  const colors = colorsFor(theme === "light");
  const id = new URLSearchParams(location.search).get("id") ?? "";
  const [url, setUrl] = useState<string | null>(null);
  const [size, setSize] = useState<{ w: number; h: number } | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const [tool, setTool] = useState<MarkTool>("box");
  const [marks, setMarks] = useState<Mark[]>([]);
  /** The pin whose note is being written on the picture. */
  const [editing, setEditing] = useState<number | null>(null);
  /** The pin under the pointer, on the picture or in the list. */
  const [lit, setLit] = useState<number | null>(null);
  const [listOpen, setListOpen] = useState(false);
  const [saving, setSaving] = useState(false);
  const stage = useRef<HTMLDivElement>(null);
  const fit = useFit(stage, size);
  useEffect(() => {
    invoke<{ url: string | null }>("inspect_request", {
      method: "picture",
      params: { id },
    }).then(
      ({ url }) => {
        if (!url) return setFailure(t("The picture could not be read."));
        const image = new Image();
        image.onload = () =>
          setSize({ w: image.naturalWidth, h: image.naturalHeight });
        image.src = url;
        setUrl(url);
      },
      (error) => setFailure(String(error)),
    );
  }, [id]);
  // The host closes the window; its page may not.
  const close = () =>
    void invoke("inspect_request", { method: "close_mark", params: {} });
  const undo = () => {
    setEditing(null);
    setLit(null);
    setMarks((list) => list.slice(0, -1));
  };
  const write = (index: number, text: string) =>
    setMarks((list) =>
      list.map((mark, at) =>
        at === index && mark.kind === "pin" ? { ...mark, text } : mark,
      ),
    );
  const drop = (index: number) => {
    setEditing(null);
    setLit(null);
    setMarks((list) => list.filter((_, at) => at !== index));
  };
  const done = async () => {
    if (!url || saving) return;
    setSaving(true);
    try {
      // A pin with no words goes, so the numbers drawn match the notes.
      const kept = marks.filter(
        (mark) => mark.kind !== "pin" || mark.text.trim(),
      );
      const png = await markedPicture(url, kept);
      const notes = kept.flatMap((mark) =>
        mark.kind === "pin" ? [mark.text] : [],
      );
      await invoke("inspect_request", {
        method: "mark",
        params: { id, png, marks: notes },
      });
    } catch (error) {
      setFailure(String(error));
      setSaving(false);
    }
  };
  const latest = useRef({ undo, done, close });
  latest.current = { undo, done, close };
  useEffect(() => {
    const key = (event: KeyboardEvent) => {
      const typing = typingIn(event.target);
      const command = event.metaKey || event.ctrlKey;
      if (command && event.key === "z" && !typing) {
        event.preventDefault();
        latest.current.undo();
      } else if (command && event.key === "Enter") {
        event.preventDefault();
        void latest.current.done();
      } else if (event.key === "Escape" && !typing) {
        latest.current.close();
      } else if (!typing && !command && !event.altKey) {
        const next = TOOL_KEYS[event.key.toLowerCase()];
        if (next) setTool(next);
      }
    };
    addEventListener("keydown", key);
    return () => removeEventListener("keydown", key);
  }, []);
  return (
    <div
      style={{
        height: "100vh",
        display: "flex",
        flexDirection: "column",
        background: colors.stage,
        color: colors.text,
        fontFamily:
          '-apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif',
        userSelect: "none",
      }}
    >
      <MarkToolbar
        tool={tool}
        choose={setTool}
        undo={undo}
        clear={() => {
          setEditing(null);
          setLit(null);
          setMarks([]);
        }}
        cancel={close}
        done={done}
        empty={marks.length === 0}
        saving={saving}
        list={{
          open: listOpen,
          count: marks.filter((mark) => mark.kind === "pin").length,
          toggle: () => setListOpen((open) => !open),
        }}
        colors={colors}
      />
      <div style={{ flex: 1, minHeight: 0, display: "flex" }}>
        <div
          ref={stage}
          style={{
            flex: 1,
            minWidth: 0,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
          }}
        >
          {failure ? (
            <span style={{ color: colors.quiet, fontSize: 13 }}>{failure}</span>
          ) : url && size && fit.w > 0 ? (
            <div
              style={{
                position: "relative",
                width: fit.w,
                height: fit.h,
                borderRadius: 4,
                boxShadow:
                  "0 0 0 1px rgba(0,0,0,0.12), 0 14px 44px rgba(0,0,0,0.32)",
              }}
            >
              <img
                src={url}
                alt=""
                draggable={false}
                style={{
                  display: "block",
                  width: "100%",
                  height: "100%",
                  borderRadius: 4,
                }}
              />
              <MarkLayer
                size={size}
                marks={marks}
                tool={tool}
                lit={lit}
                add={(mark) => {
                  // A new pin opens its note at once.
                  if (mark.kind === "pin") setEditing(marks.length);
                  setMarks((list) => [...list, mark]);
                }}
              />
              <PinNotes
                size={size}
                scale={fit.w / size.w}
                marks={marks}
                editing={editing}
                edit={setEditing}
                lit={lit}
                light={setLit}
                write={write}
                drop={drop}
                colors={colors}
              />
            </div>
          ) : null}
        </div>
        <div
          inert={!listOpen}
          style={{
            display: "flex",
            flexShrink: 0,
            width: listOpen ? 264 : 0,
            overflow: "hidden",
            transition: "width 180ms ease",
          }}
        >
          <CommentList
            marks={marks}
            lit={lit}
            light={setLit}
            write={write}
            drop={drop}
            colors={colors}
          />
        </div>
      </div>
    </div>
  );
}
