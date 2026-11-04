/* A control an agent added to the panel, drawn from its node in the layout's
 * `custom` with the panel's own parts: its type and fonts, the level and agent
 * colours, the rings and the token chart. Its clicks go through the panel's own
 * services, so opening Statistics or a session behaves as the panel's rows do. */
import {
  createContext,
  Fragment,
  useContext,
  useEffect,
  useMemo,
  useState,
  type CSSProperties,
  type ReactNode,
} from "react";
import { motion, useReducedMotionConfig } from "motion/react";
import type { PanelNode, Quota, Snapshot } from "@agent-hud/protocol";
import { useFormat } from "@agent-hud/ui/format-context";
import { HUD_METRICS } from "../../model";
import { usePanelServices } from "../../PanelContext";
import { useGround } from "../../ground";
import { useListedSessions } from "../listed-sessions";
import { QuotaRingCard } from "../QuotaRingCard";
import { SourceTokenChart } from "../TokenChartSection";
import { UsageRing } from "../../usage-meter/UsageRing";
import {
  agentColor,
  noButtonChrome,
  palette,
  tabular,
  type Logo,
} from "../theme";
import {
  collections,
  fill,
  holds,
  initialState,
  itemName,
  nextOption,
  pick,
  readsTime,
  sameAgent,
  valueOf,
  type Collections,
  type Item,
  type Scope,
} from "./custom-data";

type Node = PanelNode & Record<string, unknown>;
type Colors = ReturnType<typeof palette>;

interface Drawing {
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
}
const DrawingContext = createContext<Drawing | null>(null);

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
const ALIGN: Record<string, CSSProperties["alignItems"]> = {
  start: "flex-start",
  center: "center",
  end: "flex-end",
};
const LOGO_SIZES: Record<string, number> = { small: 14, medium: 20, large: 28 };
const TEXT: Record<string, CSSProperties> = {
  title: { fontSize: 13, fontWeight: 600, lineHeight: "18px" },
  body: { fontSize: 13, lineHeight: "17px" },
  caption: { fontSize: 11, lineHeight: "15px" },
  figure: {
    fontSize: 24,
    fontWeight: 600,
    lineHeight: "28px",
    letterSpacing: -0.4,
    ...tabular,
  },
};

/** The innermost item with a field, such as the window whose level colours a text. */
function nearest(scope: Scope, field: string): Item | undefined {
  for (let i = scope.items.length - 1; i >= 0; i--)
    if (scope.items[i].item[field] !== undefined) return scope.items[i].item;
  return undefined;
}

function levelColor(level: unknown, colors: Colors, text: boolean) {
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

function textColor(
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

/** What a click does, as the panel's own rows do it. */
function useClick(node: Node, scope: Scope) {
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

function Clickable({
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

function Children({
  nodes,
  scope,
}: {
  nodes: Node[] | undefined;
  scope: Scope;
}) {
  return (
    <>
      {(nodes ?? []).map((child, index) => (
        <Control key={child.id ?? index} node={child} scope={scope} />
      ))}
    </>
  );
}

/** One node, drawn where its `when` holds. */
function Control({ node, scope }: { node: Node; scope: Scope }) {
  const drawing = useContext(DrawingContext)!;
  // The panel's own setting: pictures, which draw no frames, turn motion off.
  const reduceMotion = useReducedMotionConfig() ?? false;
  const onClick = useClick(node, scope);
  if (node.hidden === true) return null;
  if (!holds(node.when as Record<string, unknown> | undefined, scope))
    return null;
  const children = node.children as Node[] | undefined;
  const gap = GAPS[String(node.gap ?? "small")] ?? 4;
  switch (node.type) {
    case "row":
    case "column": {
      // A row places its children along it and centres them across it; a
      // column stretches them to its width unless it says where they go.
      const align = typeof node.align === "string" ? node.align : null;
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
                  alignItems: "center",
                }
              : {
                  alignItems:
                    align && align !== "spread" ? ALIGN[align] : "stretch",
                  justifyContent:
                    align === "spread" ? "space-between" : undefined,
                }),
          }}
        >
          <Children nodes={children} scope={scope} />
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
          <Children nodes={children} scope={scope} />
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
              />
            </Fragment>
          ))}
        </>
      );
    }
    case "text": {
      const style = TEXT[String(node.style ?? "body")] ?? TEXT.body;
      const words = fill(String(node.text ?? ""), scope);
      return (
        <Clickable onClick={onClick} title={onClick ? words : undefined}>
          <motion.span
            key={words}
            initial={reduceMotion ? false : { opacity: 0.4 }}
            animate={{ opacity: 1 }}
            transition={{ duration: HUD_METRICS.duration / 2 }}
            style={{
              display: "block",
              ...style,
              color: textColor(node.color, scope, drawing),
              textAlign:
                node.align === "center"
                  ? "center"
                  : node.align === "end"
                    ? "right"
                    : "left",
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: node.style === "caption" ? "normal" : "nowrap",
            }}
          >
            {words}
          </motion.span>
        </Clickable>
      );
    }
    case "logo": {
      const agent = String(valueOf(node.agent, scope) ?? "");
      const size = LOGO_SIZES[String(node.size ?? "small")] ?? 14;
      const { Logo } = drawing;
      return (
        <Clickable
          onClick={onClick}
          style={{ width: size, height: size, flex: `0 0 ${size}px` }}
        >
          {agent && <Logo source={agent} size={size} light={drawing.light} />}
        </Clickable>
      );
    }
    case "bar": {
      const raw = valueOf(node.value, scope);
      const share = typeof raw === "number" ? Math.max(0, Math.min(1, raw)) : 0;
      const level =
        valueOf(node.level, scope) ?? nearest(scope, "level")?.level;
      return (
        <Clickable onClick={onClick} style={{ flex: 1, minWidth: 40 }}>
          <div
            style={{
              height: 4,
              borderRadius: 2,
              background: drawing.colors.track,
              overflow: "hidden",
            }}
          >
            <div
              style={{
                width: `${share * 100}%`,
                height: "100%",
                borderRadius: 2,
                background: levelColor(level, drawing.colors, false),
                transition: reduceMotion
                  ? undefined
                  : `width ${HUD_METRICS.duration}s cubic-bezier(${HUD_METRICS.ease.join(",")})`,
              }}
            />
          </div>
        </Clickable>
      );
    }
    case "rings":
    case "segments": {
      const items = node.items as Record<string, unknown> | undefined;
      const rows = pick(
        drawing.all,
        "windows",
        items?.where as Record<string, unknown> | undefined,
        items?.sort as string | undefined,
        Math.min(Number(items?.limit ?? 4), 4),
        scope,
      )
        .map((item) => item.quota)
        .filter((quota): quota is Quota => Boolean(quota));
      const size = (["small", "medium", "large"] as const).find(
        (s) => s === node.size,
      );
      if (node.legend === false) {
        const ring = { small: 40, medium: 52, large: 60 }[size ?? "medium"];
        return (
          <Clickable onClick={onClick} style={{ width: ring, height: ring }}>
            <UsageRing
              style={node.type}
              items={rows.map((row) => ({
                percent: row.usedPercent,
                color: levelColor(row.status, drawing.colors, false),
              }))}
              spec={{
                size: ring,
                width: ring / 10,
                hole: ring * 0.56,
                gap: 3,
              }}
              track={drawing.colors.track}
              reduceMotion={reduceMotion}
            />
          </Clickable>
        );
      }
      return rows.length ? (
        <Clickable onClick={onClick}>
          <QuotaRingCard
            rows={rows}
            style={node.type}
            ringSize={size}
            measure="quota"
            now={scope.now}
            light={drawing.light}
            showReset={drawing.showReset}
            onStats={onClick ? undefined : drawing.onStats}
          />
        </Clickable>
      ) : null;
    }
    case "chart": {
      const agent = valueOf(node.agent, scope);
      return (
        <Clickable onClick={onClick}>
          <SourceTokenChart
            snapshot={drawing.snapshot}
            light={drawing.light}
            Logo={drawing.Logo}
            layout={{
              period: (["day", "week", "month"] as const).find(
                (p) => p === node.period,
              ),
              agents: typeof agent === "string" && agent ? [agent] : undefined,
            }}
            active={drawing.active}
          />
        </Clickable>
      );
    }
    case "divider":
      return (
        <div
          role="separator"
          style={{ height: 1, background: drawing.colors.divider }}
        />
      );
    case "picker": {
      const state = String(node.state ?? "");
      const options = (node.options as { value: string; text: string }[]) ?? [];
      return (
        <div role="group" style={{ display: "flex", gap: 2 }}>
          {options.map((option) => {
            const chosen = scope.state[state] === option.value;
            return (
              <button
                key={option.value}
                aria-pressed={chosen}
                onClick={(event) => {
                  event.stopPropagation();
                  drawing.setState(state, option.value);
                }}
                style={{
                  ...noButtonChrome,
                  fontSize: 11,
                  lineHeight: "18px",
                  padding: "0 7px",
                  borderRadius: 9,
                  cursor: "pointer",
                  color: chosen
                    ? drawing.colors.text
                    : drawing.colors.secondary,
                  background: chosen ? drawing.colors.track : "transparent",
                  transition: `background ${HUD_METRICS.duration / 2}s, color ${HUD_METRICS.duration / 2}s`,
                }}
              >
                {fill(option.text, scope)}
              </button>
            );
          })}
        </div>
      );
    }
    default:
      return null;
  }
}

/** The time, drawn again as each minute turns while the panel is open. */
function useMinute(enabled: boolean, start: number) {
  const [now, setNow] = useState(() => (enabled ? Date.now() : start));
  useEffect(() => {
    if (!enabled) return;
    setNow(Date.now());
    let timer: ReturnType<typeof setTimeout>;
    const tick = () => {
      setNow(Date.now());
      timer = setTimeout(tick, 60_000 - (Date.now() % 60_000) + 50);
    };
    timer = setTimeout(tick, 60_000 - (Date.now() % 60_000) + 50);
    return () => clearTimeout(timer);
  }, [enabled]);
  return enabled ? now : start;
}

/** A control placed in the panel or among its blocks, with the states it declares. */
export function CustomControl({
  node,
  snapshot,
  light,
  Logo,
  active,
  showReset,
  onStats,
  divided,
}: {
  node: PanelNode;
  snapshot: Snapshot;
  light: boolean;
  Logo: Logo;
  active: boolean;
  showReset: boolean;
  onStats?: () => void;
  /** A line above it, as the panel's other sections draw one. */
  divided?: boolean;
}) {
  const colors = palette(useGround(light));
  const format = useFormat();
  const root = node as Node;
  const [state, setStates] = useState(() => initialState(root as never));
  const all = useMemo(() => collections(snapshot), [snapshot]);
  const now = useMinute(
    active && readsTime(node),
    Date.parse(snapshot.generatedAt),
  );
  const drawing: Drawing = {
    all,
    snapshot,
    colors,
    light,
    Logo,
    active,
    showReset,
    onStats,
    root,
    setState: (name, value) =>
      value === undefined
        ? undefined
        : setStates((current) => ({ ...current, [name]: value })),
  };
  const scope: Scope = {
    items: [],
    state,
    now,
    language: format.language,
    calendar: format.calendar,
  };
  return (
    <DrawingContext.Provider value={drawing}>
      <div
        data-hud-custom={node.id}
        style={{
          color: colors.text,
          minWidth: 0,
          ...(divided
            ? {
                boxShadow: `inset 0 1px 0 ${colors.divider}`,
                paddingTop: "var(--hud-divided-gap)",
              }
            : {}),
        }}
      >
        <Control node={root} scope={scope} />
      </div>
    </DrawingContext.Provider>
  );
}
