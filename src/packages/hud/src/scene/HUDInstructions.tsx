import { useT } from "@agent-hud/ui/format-context";

/** What the HUD does, for screen readers; the scene names it by `id`. */
export function HUDInstructions({ id }: { id: string }) {
  const t = useT();
  return (
    <span id={id} className="sr-only">
      {t(
        "Hover to expand. In separated agent mode, click an agent to open its full panel.",
      )}
    </span>
  );
}
