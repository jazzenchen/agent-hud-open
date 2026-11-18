/* The shortcut that calls the inspector up again after its bubble has
 * gone: ⌥Z unless the person records another, or none once cleared. One
 * field: clicked, it listens; the next combination is the new shortcut,
 * Delete clears it, Esc or a click elsewhere keeps the old one. While it
 * listens the host lets the old one go, so its keys reach this window.
 * Shown only where the system can hold a shortcut. */
import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import type { Translate } from "@agent-hud/ui/format";
import { Input } from "@agent-hud/ui/components/input";
import { Row } from "../SettingsPrimitives";

/** In the order macOS writes them. */
const MODIFIERS = [
  ["Ctrl", "⌃"],
  ["Alt", "⌥"],
  ["Shift", "⇧"],
  ["Cmd", "⌘"],
] as const;
const SIGNS: Record<string, string> = {
  Space: "Space",
  Minus: "-",
  Equal: "=",
  BracketLeft: "[",
  BracketRight: "]",
  Backslash: "\\",
  Semicolon: ";",
  Quote: "'",
  Comma: ",",
  Period: ".",
  Slash: "/",
  Backquote: "`",
};

/** A key the host can hold, by its `KeyboardEvent.code`, as written. */
function keyLabel(code: string): string | null {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit[0-9]$/.test(code)) return code.slice(5);
  if (/^F([1-9]|1[0-2])$/.test(code)) return code;
  return SIGNS[code] ?? null;
}

/** `Alt+KeyZ` as macOS writes it: ⌥Z. */
export function shortcutLabel(text: string): string {
  const parts = text.split("+");
  const code = parts.pop() ?? "";
  const signs = MODIFIERS.filter(([name]) => parts.includes(name))
    .map(([, sign]) => sign)
    .join("");
  return signs + (keyLabel(code) ?? code);
}

/** A key press as a shortcut: none while only modifiers are down, for a key
 * the host cannot hold, or without ⌘, ⌃ or ⌥ (it would take the key from
 * typing everywhere). */
function shortcutOf(event: KeyboardEvent<HTMLInputElement>): string | null {
  if (!keyLabel(event.code)) return null;
  if (!event.metaKey && !event.ctrlKey && !event.altKey) return null;
  const modifiers = [
    event.metaKey && "Cmd",
    event.ctrlKey && "Ctrl",
    event.altKey && "Alt",
    event.shiftKey && "Shift",
  ].filter(Boolean);
  return [...modifiers, event.code].join("+");
}

const ask = (method: string, params: object = {}) =>
  invoke<{ supported: boolean }>("inspect_request", { method, params });

export function ShortcutRow({
  value,
  change,
  t,
}: {
  value: string;
  change: (value: string) => void;
  t: Translate;
}) {
  const [supported, setSupported] = useState(false);
  const [recording, setRecording] = useState(false);
  const field = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (!isTauri()) return;
    ask("shortcut").then(
      (status) => setSupported(status.supported),
      () => {},
    );
  }, []);
  useEffect(() => {
    if (!recording) return;
    ask("hold_shortcut", { recording: true }).catch(() => {});
    return () => {
      ask("hold_shortcut", { recording: false }).catch(() => {});
    };
  }, [recording]);
  const key = (event: KeyboardEvent<HTMLInputElement>) => {
    // Tab moves on as anywhere else, and stops listening on the way.
    if (event.key === "Tab") return;
    event.preventDefault();
    event.stopPropagation();
    const bare = !event.metaKey && !event.ctrlKey && !event.altKey;
    if (event.key === "Escape") return field.current?.blur();
    if (bare && (event.key === "Backspace" || event.key === "Delete")) {
      change("");
      return field.current?.blur();
    }
    const next = shortcutOf(event);
    if (!next) return;
    change(next);
    field.current?.blur();
  };
  if (!supported) return null;
  return (
    <Row
      title={t("Shortcut")}
      subtitle={
        recording
          ? t(
              "Press the new shortcut. Delete clears it, Esc keeps the old one.",
            )
          : t("Opens the inspector again after its bubble has gone.")
      }
    >
      <Input
        ref={field}
        readOnly
        aria-label={t("Shortcut")}
        value={recording ? "" : value ? shortcutLabel(value) : ""}
        placeholder={recording ? t("Press keys…") : t("None")}
        onFocus={() => setRecording(true)}
        onBlur={() => setRecording(false)}
        onKeyDown={key}
        className="h-[26px] cursor-default rounded-[6px] px-2 py-0 text-center text-xs md:text-xs"
        style={{ width: 120, caretColor: "transparent" }}
      />
    </Row>
  );
}
