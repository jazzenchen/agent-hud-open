import {
  Fragment,
  useContext,
  useLayoutEffect,
  useRef,
  useState,
  type CSSProperties,
} from "react";
import { itemName } from "./custom-collections";
import { heldSections, holds, pick, SECTIONS, type Scope } from "./custom-data";
import { Clickable, useClick } from "./Clickable";
import { card } from "../theme";
import { DrawingContext, type Node } from "./drawing";
import { LeafControl } from "./LeafControl";
import { placing, spanning } from "./placing";

const GAPS: Record<string, number> = {
  none: 0,
  small: 4,
  medium: 8,
  large: 12,
};
const JUSTIFY: Record<string, CSSProperties["justifyContent"]> = {
  start: "flex-start",
  center: "center",
  end: "flex-end",
  spread: "space-between",
};
const VALIGN: Record<string, CSSProperties["alignItems"]> = {
  top: "flex-start",
  center: "center",
  bottom: "flex-end",
};
/** What shares a row's room with the others, as a bar takes what is left. */
const SHARING = ["column", "grid", ...SECTIONS];
const ALIGN: Record<string, CSSProperties["alignItems"]> = {
  start: "flex-start",
  center: "center",
  end: "flex-end",
};

function Children({
  nodes,
  scope,
  parent,
}: {
  nodes: Node[] | undefined;
  scope: Scope;
  parent: string;
}) {
  // Beside a section, which is a card, the other parts are cards too.
  const carded = (nodes ?? []).some((child) =>
    SECTIONS.includes(String(child.type)),
  );
  return (
    <>
      {(nodes ?? []).map((child, index) => (
        <Placed
          key={child.id ?? index}
          node={child}
          scope={scope}
          parent={parent}
          index={index}
          carded={carded}
        />
      ))}
    </>
  );
}

export function Placed({
  node,
  scope,
  parent,
  index,
  carded = false,
}: {
  node: Node;
  scope: Scope;
  parent: string;
  index: number;
  /** A card of its own, as the sections beside it are. */
  carded?: boolean;
}) {
  const drawing = useContext(DrawingContext)!;
  if (SECTIONS.includes(String(node.type)))
    return <Slot node={node} parent={parent} index={index} />;
  // What holds sections is no card itself, and between cards no line is drawn.
  const inCard = carded && drawing.separated && heldSections(node).length === 0;
  if (inCard && node.type === "divider") return null;
  const control = <Control node={node} scope={scope} parent={parent} />;
  const placed = placing(
    node.width,
    parent === "row" && SHARING.includes(String(node.type)),
  );
  const spans = spanning(node, parent);
  const place = placed || spans ? { ...placed, ...spans } : null;
  if (inCard)
    return (
      <div
        className="hud-card"
        style={{ minWidth: 0, ...place, ...card(drawing.colors) }}
      >
        {control}
      </div>
    );
  if (
    !place ||
    node.hidden === true ||
    !holds(node.when as Record<string, unknown> | undefined, scope)
  )
    return control;
  // A column of cards takes the height its row gives it.
  const filled =
    parent === "row" && drawing.separated
      ? { display: "grid", gridTemplateColumns: "minmax(0, 1fr)" }
      : null;
  return <div style={{ ...filled, ...place }}>{control}</div>;
}

/** One of the panel's own sections where a control of an agent's holds it: drawn
 * by the panel at the width it is given here, as a card of its own; where the
 * panel's style asks for no cards, with no line above it where it heads a column. */
function Slot({
  node,
  parent,
  index,
}: {
  node: Node;
  parent: string;
  index: number;
}) {
  const drawing = useContext(DrawingContext)!;
  const element = useRef<HTMLDivElement>(null);
  const [measured, setMeasured] = useState<number>();
  const section = String(node.type);
  // In a card a section draws no line above itself.
  const bare = drawing.separated || parent !== "column" || index === 0;
  const content = drawing.section?.(section, measured, bare);
  const shown = Boolean(content);
  useLayoutEffect(() => {
    const box = element.current;
    if (!box) return;
    const measure = () =>
      setMeasured(
        box.getBoundingClientRect().width -
          (parseFloat(getComputedStyle(box).paddingLeft) || 0) -
          (parseFloat(getComputedStyle(box).paddingRight) || 0),
      );
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(box);
    return () => observer.disconnect();
  }, [shown]);
  if (!content) return null;
  return (
    <div
      ref={element}
      data-hud-section={section}
      style={{
        minWidth: 0,
        ...placing(node.width, parent === "row"),
        // A card fills the height its row gives it, and in a column takes
        // the room the others leave; the section fills the card.
        ...(drawing.separated
          ? {
              ...card(drawing.colors),
              display: "grid",
              ...(parent === "column" ? { flexGrow: 1 } : {}),
            }
          : {}),
      }}
    >
      {content}
    </div>
  );
}

/** One node, drawn where its `when` holds. */
function Control({
  node,
  scope,
  parent,
}: {
  node: Node;
  scope: Scope;
  parent: string;
}) {
  const drawing = useContext(DrawingContext)!;
  const onClick = useClick(node, scope);
  if (node.hidden === true) return null;
  if (!holds(node.when as Record<string, unknown> | undefined, scope))
    return null;
  const children = node.children as Node[] | undefined;
  const gap = GAPS[String(node.gap ?? "small")] ?? 4;
  switch (node.type) {
    case "row":
    case "column": {
      // A row places its children along it and centres them across it, or
      // tops them when it holds a section, column or grid; a column stretches
      // them to its width unless it says where they go.
      const align = typeof node.align === "string" ? node.align : null;
      const tall = (children ?? []).some((child) =>
        SHARING.includes(String(child.type)),
      );
      return (
        <Clickable
          onClick={onClick}
          style={{
            display: "flex",
            flexDirection: node.type === "row" ? "row" : "column",
            gap,
            minWidth: 0,
            ...(node.type === "row"
              ? {
                  justifyContent: JUSTIFY[align ?? "start"] ?? "flex-start",
                  alignItems:
                    VALIGN[String(node.valign)] ??
                    // Cards side by side stand at one height.
                    (tall
                      ? drawing.separated
                        ? "stretch"
                        : "flex-start"
                      : "center"),
                }
              : {
                  alignItems:
                    align && align !== "spread" ? ALIGN[align] : "stretch",
                  justifyContent:
                    align === "spread" ? "space-between" : undefined,
                }),
          }}
        >
          <Children nodes={children} scope={scope} parent={node.type} />
        </Clickable>
      );
    }
    case "grid": {
      const columns = Math.max(2, Math.min(6, Number(node.columns ?? 2)));
      return (
        <Clickable
          onClick={onClick}
          style={{
            display: "grid",
            gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))`,
            // A cell that spans more leaves gaps the smaller ones after it fill.
            gridAutoFlow: "dense",
            gap,
          }}
        >
          <Children nodes={children} scope={scope} parent="grid" />
        </Clickable>
      );
    }
    case "repeat": {
      const over = String(node.over ?? "");
      const items = pick(
        drawing.all,
        over,
        node.where as Record<string, unknown> | undefined,
        node.sort as string | undefined,
        node.limit as number | undefined,
        scope,
      );
      const name = itemName(over);
      // Each item's controls sit in the repeat's parent, as if written there.
      return (
        <>
          {items.map((item, index) => (
            <Fragment key={String(item.id ?? index)}>
              <Children
                nodes={children}
                scope={{ ...scope, items: [...scope.items, { name, item }] }}
                parent={parent}
              />
            </Fragment>
          ))}
        </>
      );
    }
    default:
      return (
        <LeafControl
          node={node}
          scope={scope}
          onClick={onClick}
          parent={parent}
        />
      );
  }
}
