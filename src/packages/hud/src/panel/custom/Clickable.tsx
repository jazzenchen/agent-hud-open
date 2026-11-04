import {
  useContext,
  useState,
  type CSSProperties,
  type ReactNode,
} from "react";
import { HUD_METRICS } from "../../model";
import { usePanelServices } from "../../PanelContext";
import { useListedSessions } from "../listed-sessions";
import { nextOption, valueOf, type Scope } from "./custom-data";
import { DrawingContext, type Node } from "./drawing";

/** What a click does, as the panel's own rows do it. */
export function useClick(node: Node, scope: Scope) {
  const drawing = useContext(DrawingContext)!;
  const services = usePanelServices();
  const listed = useListedSessions();
  const click = node.onClick as Record<string, unknown> | undefined;
  if (!click) return undefined;
  return () => {
    const value = (field: string) => {
      const read = valueOf(click[field], scope);
      return read == null || read === "" ? undefined : String(read);
    };
    if (click.open === "statistics" || click.open === "session") {
      const sessionId = value("session");
      const agent = value("agent");
      if (sessionId) {
        if (listed) return listed.openStats({ sessionId });
        const session = drawing.snapshot.sessions.find(
          (s) => s.id === sessionId,
        );
        if (session && services?.selectSession)
          return services.selectSession(session);
        return drawing.onStats?.();
      }
      if (agent) services?.selectProvider?.(agent);
      const tab = click.tab === "sessions" ? "sessions" : "tokens";
      if (listed && !agent) return listed.openStats({ tab });
      return drawing.onStats?.();
    }
    if (click.open === "settings") {
      const page = value("page");
      if (page) services?.openSettingsPage?.(page);
      return;
    }
    if (typeof click.set === "string")
      return drawing.setState(click.set, value("to"));
    if (typeof click.next === "string")
      return drawing.setState(
        click.next,
        nextOption(drawing.root as never, click.next, scope.state[click.next]),
      );
    if ("copy" in click) {
      const text = value("copy");
      if (text) void navigator.clipboard?.writeText(text).catch(() => {});
    }
  };
}

export function Clickable({
  onClick,
  title,
  style,
  children,
}: {
  onClick?: () => void;
  title?: string;
  style?: CSSProperties;
  children: ReactNode;
}) {
  const [hovered, setHovered] = useState(false);
  if (!onClick) return <div style={style}>{children}</div>;
  return (
    <div
      role="button"
      tabIndex={0}
      title={title}
      onClick={(event) => {
        event.stopPropagation();
        onClick();
      }}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onClick();
        }
      }}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      style={{
        ...style,
        cursor: "pointer",
        borderRadius: 6,
        opacity: hovered ? 0.72 : 1,
        transition: `opacity ${HUD_METRICS.duration / 2}s`,
      }}
    >
      {children}
    </div>
  );
}
