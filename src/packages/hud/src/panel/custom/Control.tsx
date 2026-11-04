import {
  Fragment,
  useContext,
  useLayoutEffect,
  useRef,
  useState,
  type CSSProperties,
} from "react";
import { holds, itemName, pick, SECTIONS, type Scope } from "./custom-data";
import { Clickable, useClick } from "./Clickable";
import { DrawingContext, type Colors, type Node } from "./drawing";
import { LeafControl } from "./LeafControl";

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
  lined = false,
}: {
  nodes: Node[] | undefined;
  scope: Scope;
  parent: string;
  /** A line between the children, as between sections side by side. */
  lined?: boolean;
}) {
  return (
    <>
      {(nodes ?? []).map((child, index) => (
        <Placed
          key={child.id ?? index}
          node={child}
          scope={scope}
          parent={parent}
          index={index}
          lined={lined && index > 0}
        />
      ))}
    </>
  );
}

/** The line beside a part that follows another in a row of sections. */
const besideLine = (colors: Colors): CSSProperties => ({
  boxShadow: `inset 1px 0 0 ${colors.divider}`,
  paddingLeft: "var(--hud-section-gap)",
});

/** Where a control sits in its parent: a width of its own, or in a row an equal
 * share of the room left for a column, a grid or a section. */
function placing(width: unknown, shares: boolean): CSSProperties | null {
  if (typeof width === "number")
    return { flex: `0 0 ${width}px`, width, minWidth: 0, maxWidth: "100%" };
  return shares ? { flex: "1 1 0", minWidth: 0 } : null;
}

export function Placed({
  node,
  scope,
  parent,
  index,
  lined = false,
}: {
  node: Node;
  scope: Scope;
  parent: string;
  index: number;
  lined?: boolean;
}) {
  const drawing = useContext(DrawingContext)!;
  if (SECTIONS.includes(String(node.type)))
    return <Slot node={node} parent={parent} index={index} lined={lined} />;
  const control = <Control node={node} scope={scope} parent={parent} />;
  const place = placing(
    node.width,
    parent === "row" && SHARING.includes(String(node.type)),
  );
  if (
    (!place && !lined) ||
    node.hidden === true ||
    !holds(node.when as Record<string, unknown> | undefined, scope)
  )
    return control;
  return (
    <div style={{ ...place, ...(lined ? besideLine(drawing.colors) : {}) }}>
      {control}
    </div>
  );
}

/** One of the panel's own sections where a control of an agent's holds it: drawn
 * by the panel at the width it is given here, with no line above it where it heads
 * a column, and a line beside it after another in a row. */
function Slot({
  node,
  parent,
  index,
  lined,
}: {
  node: Node;
  parent: string;
  index: number;
  lined: boolean;
}) {
  const drawing = useContext(DrawingContext)!;
  const element = useRef<HTMLDivElement>(null);
  const [measured, setMeasured] = useState<number>();
  const section = String(node.type);
  const bare = parent !== "column" || index === 0;
  const content = drawing.section?.(section, measured, bare);
  const shown = Boolean(content);
  useLayoutEffect(() => {
    const box = element.current;
    if (!box) return;
    const measure = () =>
      setMeasured(
        box.getBoundingClientRect().width -
          (parseFloat(getComputedStyle(box).paddingLeft) || 0),
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
        ...(lined ? besideLine(drawing.colors) : {}),
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
                    (tall ? "flex-start" : "center"),
                }
              : {
                  alignItems:
                    align && align !== "spread" ? ALIGN[align] : "stretch",
                  justifyContent:
                    align === "spread" ? "space-between" : undefined,
                }),
          }}
        >
          <Children
            nodes={children}
            scope={scope}
            parent={node.type}
            lined={
              node.type === "row" &&
              drawing.separated &&
              (children ?? []).some((child) =>
                SECTIONS.includes(String(child.type)),
              )
            }
          />
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
      return <LeafControl node={node} scope={scope} onClick={onClick} />;
  }
}
