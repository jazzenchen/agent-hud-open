// What every control of an agent's own draws with: the snapshot's
// collections, the panel's colours and parts, the states it declares, and the
// panel's own sections where a row, column or grid of it holds one.
import { createContext, type ReactNode } from "react";
import type { PanelNode, Snapshot } from "@agent-hud/protocol";
import { agentColor, palette, type Logo } from "../theme";
import { sameAgent, type Collections, type Item } from "./custom-collections";
import type { Scope } from "./custom-data";

export type Node = PanelNode & Record<string, unknown>;
export type Colors = ReturnType<typeof palette>;

export interface Drawing {
  all: Collections;
  snapshot: Snapshot;
  colors: Colors;
  light: boolean;
  Logo: Logo;
  active: boolean;
  showReset: boolean;
  onStats?: () => void;
  root: Node;
  setState(name: string, value: string | undefined): void;
  /** One of the panel's own sections, drawn in a row, column or grid of this control. */
  section?: (
    key: string,
    width: number | undefined,
    bare: boolean,
  ) => ReactNode;
  separated: boolean;
}
export const DrawingContext = createContext<Drawing | null>(null);

/** The innermost item with a field, such as the window whose level colours a text. */
export function nearest(scope: Scope, field: string): Item | undefined {
  for (let i = scope.items.length - 1; i >= 0; i--)
    if (scope.items[i].item[field] !== undefined) return scope.items[i].item;
  return undefined;
}

export function levelColor(level: unknown, colors: Colors, text: boolean) {
  switch (level) {
    case "critical":
      return text ? colors.criticalText : colors.critical;
    case "warning":
      return text ? colors.warningText : colors.warning;
    case "stale":
      return colors.secondary;
    default:
      return text ? colors.okText : colors.ok;
  }
}

export function textColor(
  color: unknown,
  scope: Scope,
  drawing: Drawing,
): string | undefined {
  const { colors } = drawing;
  switch (color) {
    case "secondary":
      return colors.secondary;
    case "tertiary":
      return colors.tertiary;
    case "level":
      return levelColor(nearest(scope, "level")?.level, colors, true);
    case "agent": {
      const agent = nearest(scope, "agent")?.agent ?? nearest(scope, "id")?.id;
      const index = drawing.snapshot.providers.findIndex((p) =>
        sameAgent(p.id, agent),
      );
      return index < 0 ? colors.secondary : agentColor(index, drawing.light);
    }
    case "ok":
      return colors.okText;
    case "warning":
      return colors.warningText;
    case "critical":
      return colors.criticalText;
    default:
      return colors.text;
  }
}
