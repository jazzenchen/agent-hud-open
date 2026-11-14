import { ChevronDown, ChevronUp } from "lucide-react";
import { Button } from "@agent-hud/ui/components/button";
import type { Translate } from "@agent-hud/ui/format";

/** Move-up and move-down for one row of an ordered list. */
export function ReorderButtons({
  name,
  index,
  count,
  move,
  t,
}: {
  name: string;
  index: number;
  count: number;
  move: (by: -1 | 1) => void;
  t: Translate;
}) {
  return (
    <>
      <Button
        variant="ghost"
        size="icon-xs"
        disabled={index === 0}
        aria-label={t("Move {{name}} up", { name })}
        title={t("Move up")}
        onClick={() => move(-1)}
      >
        <ChevronUp size={13} />
      </Button>
      <Button
        variant="ghost"
        size="icon-xs"
        disabled={index === count - 1}
        aria-label={t("Move {{name}} down", { name })}
        title={t("Move down")}
        onClick={() => move(1)}
      >
        <ChevronDown size={13} />
      </Button>
    </>
  );
}
