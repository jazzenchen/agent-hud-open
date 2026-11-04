import { useCallback, useState, type RefObject } from "react";

/** Whether an editable text field inside the scene holds keyboard focus;
 * `checkDomTyping` looks again as focus moves. */
export function useDomTyping(scene: RefObject<HTMLElement | null>) {
  const [domTyping, setDomTyping] = useState(false);
  const checkDomTyping = useCallback(() => {
    const active = document.activeElement;
    if (
      !(active instanceof HTMLInputElement) &&
      !(active instanceof HTMLTextAreaElement)
    ) {
      setDomTyping(false);
      return;
    }
    if (
      active.disabled ||
      active.readOnly ||
      !scene.current?.contains(active) ||
      active.closest("[inert], [aria-hidden=true]")
    ) {
      setDomTyping(false);
      return;
    }
    setDomTyping(true);
  }, []);
  return { domTyping, checkDomTyping };
}
