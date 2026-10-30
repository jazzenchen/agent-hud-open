import { ChevronDown, ChevronUp } from "lucide-react";
import { Button } from "@agent-hud/ui/components/button";
import type { Text } from "../SettingsPrimitives";

/** Swaps `id` with its neighbour among `visible`, inside the full `order`
 * (which may keep ids that are not on screen, so their places survive a
 * failed read). Returns `order` unchanged at either end. */
export function moveAmong(
  order: string[],
  visible: string[],
  id: string,
  direction: -1 | 1,
) {
  const at = visible.indexOf(id);
  const neighbour = visible[at + direction];
  if (at < 0 || !neighbour) return order;
  const next = [...order];
  const left = next.indexOf(id),
    right = next.indexOf(neighbour);
  if (left < 0 || right < 0) return order;
  [next[left], next[right]] = [next[right], next[left]];
  return next;
}

/** Up and down icon buttons for one item of a list. `name` finishes the
 * buttons' spoken labels ("Move up Current session"). */
export function ReorderButtons({
  t,
  name,
  first,
  last,
  disabled = false,
  move,
}: {
  t: Text;
  name: string;
  first: boolean;
  last: boolean;
  disabled?: boolean;
  move: (direction: -1 | 1) => void;
}) {
  return (
    <>
      <Button
        variant="ghost"
        size="icon-xs"
        aria-label={`${t("Move up", "上移")} ${name}`}
        disabled={disabled || first}
        onClick={() => move(-1)}
      >
        <ChevronUp size={13} />
      </Button>
      <Button
        variant="ghost"
        size="icon-xs"
        aria-label={`${t("Move down", "下移")} ${name}`}
        disabled={disabled || last}
        onClick={() => move(1)}
      >
        <ChevronDown size={13} />
      </Button>
    </>
  );
}
