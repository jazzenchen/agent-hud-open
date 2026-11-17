import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useT } from "@agent-hud/ui/format-context";
import { MarkLayer } from "./MarkLayer";
import { MarkToolbar } from "./MarkToolbar";
import { PinNotes } from "./PinNotes";
import { markedPicture, type Mark, type MarkTool } from "./mark-model";

export interface MarkColors {
  ground: string;
  stage: string;
  text: string;
  quiet: string;
  line: string;
  chosen: string;
  field: string;
}

const colorsFor = (light: boolean): MarkColors =>
  light
    ? {
        ground: "#F5F5F7",
        stage: "#E8E8ED",
        text: "#1C1C1E",
        quiet: "rgba(0,0,0,0.5)",
        line: "rgba(0,0,0,0.1)",
        chosen: "rgba(0,0,0,0.08)",
        field: "#FFFFFF",
      }
    : {
        ground: "#1C1C1E",
        stage: "#111113",
        text: "#F2F2F7",
        quiet: "rgba(255,255,255,0.55)",
        line: "rgba(255,255,255,0.1)",
        chosen: "rgba(255,255,255,0.12)",
        field: "rgba(255,255,255,0.06)",
      };

/** The size the picture shows at: as large as fits `stage`, never past
 * twice its own pixels. */
function useFit(
  stage: React.RefObject<HTMLDivElement | null>,
  size: { w: number; h: number } | null,
) {
  const [fit, setFit] = useState({ w: 0, h: 0 });
  useLayoutEffect(() => {
    const element = stage.current;
    if (!element || !size) return;
    const measure = () => {
      const room = element.getBoundingClientRect();
      const scale = Math.min(
        (room.width - 32) / size.w,
        (room.height - 32) / size.h,
        2,
      );
      setFit({ w: size.w * scale, h: size.h * scale });
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, [size?.w, size?.h]);
  return fit;
}

/** The markup window: a picture from the clipboard window, large, with a
 * box, a pen and numbered comments. Done sends the picture with its marks
 * drawn in, and the comments, back to the clipboard window's draft. */
export default function MarkWindow({ theme }: { theme: "dark" | "light" }) {
  const t = useT();
  const colors = colorsFor(theme === "light");
  const id = new URLSearchParams(location.search).get("id") ?? "";
  const [url, setUrl] = useState<string | null>(null);
  const [size, setSize] = useState<{ w: number; h: number } | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const [tool, setTool] = useState<MarkTool>("box");
  const [marks, setMarks] = useState<Mark[]>([]);
  /** The pin whose note is being written. */
  const [editing, setEditing] = useState<number | null>(null);
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
  const undo = () => setMarks((list) => list.slice(0, -1));
  useEffect(() => {
    const key = (event: KeyboardEvent) => {
      const typing = event.target instanceof HTMLInputElement;
      if ((event.metaKey || event.ctrlKey) && event.key === "z" && !typing) {
        event.preventDefault();
        undo();
      } else if (event.key === "Escape") close();
    };
    addEventListener("keydown", key);
    return () => removeEventListener("keydown", key);
  }, []);
  const done = async () => {
    if (!url || saving) return;
    setSaving(true);
    try {
      const png = await markedPicture(url, marks);
      const notes = marks.flatMap((mark) =>
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
  return (
    <div
      style={{
        height: "100vh",
        display: "flex",
        flexDirection: "column",
        background: colors.ground,
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
        clear={() => setMarks([])}
        cancel={close}
        done={done}
        saving={saving}
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
            background: colors.stage,
          }}
        >
          {failure ? (
            <span style={{ color: colors.quiet, fontSize: 13 }}>{failure}</span>
          ) : url && size && fit.w > 0 ? (
            <div style={{ position: "relative", width: fit.w, height: fit.h }}>
              <img
                src={url}
                alt=""
                draggable={false}
                style={{ display: "block", width: "100%", height: "100%" }}
              />
              <MarkLayer
                size={size}
                marks={marks}
                tool={tool}
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
                write={(index, text) =>
                  setMarks((list) =>
                    list.map((mark, at) =>
                      at === index && mark.kind === "pin"
                        ? { ...mark, text }
                        : mark,
                    ),
                  )
                }
                drop={(index) =>
                  setMarks((list) => list.filter((_, at) => at !== index))
                }
                colors={colors}
              />
            </div>
          ) : null}
        </div>
      </div>
    </div>
  );
}
