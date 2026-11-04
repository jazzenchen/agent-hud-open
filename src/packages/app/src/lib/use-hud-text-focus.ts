import { useEffect, useRef, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import type { Approval } from "@agent-hud/protocol";

/** Whether the HUD holds the keyboard for its text: the HUD window takes the
 * keyboard while one of its text fields, or words selected in it, are in use,
 * and gives it back when that ends, when the request a field belongs to goes,
 * or when the pointer leaves the HUD. */
export function useHUDTextFocus(
  screenId: string,
  approvals: Approval[],
  pointerInside: boolean | undefined,
) {
  const [textFocused, setTextFocused] = useState(false);
  const liveRequests = useRef(new Set<string>());
  liveRequests.current = new Set(approvals.map((request) => request.id));
  const requestIdentity = approvals.map((request) => request.id).join("\0");
  const textFocusOwner = useRef<{
    verify: () => void;
    leave: () => void;
  } | null>(null);
  useEffect(() => {
    if (!isTauri()) return;
    type TextControl = HTMLInputElement | HTMLTextAreaElement;
    const editable = (target: EventTarget | null): TextControl | null => {
      if (!(target instanceof Element)) return null;
      const element = target.closest("textarea, input");
      if (
        !(element instanceof HTMLTextAreaElement) &&
        !(element instanceof HTMLInputElement)
      )
        return null;
      if (
        element instanceof HTMLInputElement &&
        ![
          "text",
          "search",
          "email",
          "url",
          "tel",
          "password",
          "number",
        ].includes(element.type)
      )
        return null;
      if (
        element.disabled ||
        element.readOnly ||
        element.closest("[inert], [aria-hidden=true]")
      )
        return null;
      return element;
    };
    let element: TextControl | null = null;
    let wanted = false,
      disposed = false,
      epoch = 0;
    // Words selected outside a text field also take the keyboard, so ⌘C
    // copies them instead of reaching the app in front. Unlike typing, this
    // leaves the panel's opening and closing alone.
    let selecting = false,
      pressed = false;
    const selected = (): Element | null => {
      const selection = document.getSelection();
      if (!selection || selection.isCollapsed || !selection.rangeCount)
        return null;
      const node = selection.getRangeAt(0).commonAncestorContainer;
      const owner = node instanceof Element ? node : node.parentElement;
      if (
        !owner?.isConnected ||
        owner.closest("[inert], [aria-hidden=true]") ||
        editable(owner)
      )
        return null;
      return owner;
    };
    let serial: Promise<void> = Promise.resolve();
    // A release carries this surface's identity, so a late unmount cannot clear
    // another surface's newer lease after React remounts it.
    const leaseId = crypto.randomUUID();
    const release = () => {
      if (!wanted) return;
      wanted = false;
      selecting = false;
      setTextFocused(false);
      changed.disconnect();
      epoch += 1;
      const previous = element;
      element = null;
      if (document.activeElement === previous) previous?.blur();
      serial = serial
        .catch(() => undefined)
        .then(async () => {
          await invoke("hud_text_focus", { active: false, leaseId });
        })
        .catch((error) => {
          if (!disposed)
            console.error("HUD keyboard focus release failed", error);
        });
    };
    const verified = (target: TextControl | null) => {
      if (!target?.isConnected || target.closest("[inert], [aria-hidden=true]"))
        return false;
      const requestId =
        target.closest<HTMLElement>("[data-request-id]")?.dataset.requestId;
      return !requestId || liveRequests.current.has(requestId);
    };
    const owner = {
      verify: () => {
        if (!wanted || (selecting && pressed)) return;
        if (selecting ? !selected() : !verified(element)) release();
      },
      // The pointer left the HUD: its panel closes, so the selection goes
      // and the keys go back.
      leave: () => {
        if (!selecting) return;
        document.getSelection()?.removeAllRanges();
        release();
      },
    };
    textFocusOwner.current = owner;
    const acquire = (target: TextControl, refocus: boolean) => {
      element = target;
      wanted = true;
      selecting = false;
      setTextFocused(true);
      const root = document.getElementById("root");
      if (root) changed.observe(root, { childList: true, subtree: true });
      const captured = ++epoch;
      serial = serial
        .catch(() => undefined)
        .then(async () => {
          if (disposed || !wanted || captured !== epoch || !verified(target))
            return;
          await invoke("hud_text_focus", { active: true, leaseId });
          if (
            refocus &&
            !disposed &&
            wanted &&
            captured === epoch &&
            verified(target)
          )
            target.focus({ preventScroll: true });
        })
        .catch((error) => {
          if (!disposed && captured === epoch) {
            console.error("HUD keyboard focus failed", error);
            release();
          }
        });
    };
    const holdSelection = () => {
      if (wanted || !selected()) return;
      wanted = true;
      selecting = true;
      const root = document.getElementById("root");
      if (root) changed.observe(root, { childList: true, subtree: true });
      const captured = ++epoch;
      serial = serial
        .catch(() => undefined)
        .then(async () => {
          if (
            disposed ||
            !wanted ||
            !selecting ||
            captured !== epoch ||
            !selected()
          )
            return;
          await invoke("hud_text_focus", { active: true, leaseId });
        })
        .catch((error) => {
          if (!disposed && captured === epoch) {
            console.error("HUD keyboard focus failed", error);
            release();
          }
        });
    };
    const pointerDown = (event: PointerEvent) => {
      if (!event.isTrusted || event.button !== 0) return;
      pressed = true;
      const target = editable(event.target);
      if (!target || !verified(target)) {
        // A press may start a new selection: a selection's keys wait for it
        // to end rather than going back mid-drag.
        if (!selecting) release();
        return;
      }
      acquire(target, true);
    };
    const pointerUp = (event: PointerEvent) => {
      if (!event.isTrusted || event.button !== 0) return;
      pressed = false;
      if (selected()) holdSelection();
      else if (selecting) release();
    };
    const selectionChange = () => {
      if (selecting && !pressed && !selected()) release();
    };
    const focusIn = (event: FocusEvent) => {
      const target = editable(event.target);
      if (!target || !verified(target)) return;
      if (!wanted || element !== target) acquire(target, false);
    };
    const focusOut = () =>
      queueMicrotask(() => {
        if (disposed || !wanted || selecting) return;
        const next = editable(document.activeElement);
        if (next && verified(next)) element = next;
        else release();
      });
    const changed = new MutationObserver(owner.verify);
    document.addEventListener("pointerdown", pointerDown, true);
    document.addEventListener("pointerup", pointerUp, true);
    document.addEventListener("selectionchange", selectionChange);
    document.addEventListener("focusin", focusIn, true);
    document.addEventListener("focusout", focusOut, true);
    window.addEventListener("blur", release);
    window.addEventListener("pagehide", release);
    return () => {
      disposed = true;
      release();
      changed.disconnect();
      document.removeEventListener("pointerdown", pointerDown, true);
      document.removeEventListener("pointerup", pointerUp, true);
      document.removeEventListener("selectionchange", selectionChange);
      document.removeEventListener("focusin", focusIn, true);
      document.removeEventListener("focusout", focusOut, true);
      window.removeEventListener("blur", release);
      window.removeEventListener("pagehide", release);
      if (textFocusOwner.current === owner) textFocusOwner.current = null;
    };
  }, [screenId]);
  useEffect(() => {
    textFocusOwner.current?.verify();
  }, [requestIdentity]);
  useEffect(() => {
    if (pointerInside === false) textFocusOwner.current?.leave();
  }, [pointerInside]);
  return textFocused;
}
