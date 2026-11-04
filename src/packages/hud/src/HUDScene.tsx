import {
  memo,
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
} from "react";
import { useReducedMotion } from "motion/react";
import type { Snapshot } from "@agent-hud/protocol";
import { HUDArtwork as HUDLogo } from "./HUDArtwork";
export { HUDArtwork as HUDLogo } from "./HUDArtwork";
import { DockMemory, type Direction } from "./bots/mark-director";
import { useIslandNews, NewsContent, PendingEventRows } from "./IslandNews";
import {
  ApprovalContent,
  RequestDeliveryNotice,
} from "./permissions/ApprovalContent";
import { HUDExpandedSurface } from "./scene/HUDExpandedSurface";
import { HUDMarkStrip } from "./scene/HUDMarkStrip";
import { FlightLayer, type HUDFlightCommand } from "./scene/FlightLayer";
import { NotchGlow } from "./scene/NotchSurface";
import {
  useHUDDrag,
  type HUDDragPreview,
  type HUDDragStartArgs,
} from "./scene/useHUDDrag";
import {
  useHUDInteraction,
  type HUDNativePointer,
} from "./scene/useHUDInteraction";
import { useHUDLayout } from "./scene/useHUDLayout";
import { PanelServicesProvider, usePanelServices } from "./PanelContext";
import { SourceHoverPanel } from "./HoverPanel";
import { stripMetrics, type HUDRect } from "./geometry";
import { useHoverMachine } from "./hover";
import {
  defaultHUDConfig,
  HUD_METRICS,
  statusColor,
  configuredVendors,
  type HUDConfig,
  type HUDDecision,
  type HUDUpdateNotice,
  vendorKey,
} from "./model";
import { glowAlert } from "./HUDGlow";
import { InlineEventRow } from "./alerts/InlineEventRow";
import { snapshotLanguage } from "@agent-hud/ui/format";
import {
  ListedSessionsContext,
  type ListedSessions,
  type StatsTarget,
} from "./panel/listed-sessions";
import { HUDGroundProvider } from "./ground";

export type {
  HUDConfig,
  HUDDecision,
  HUDEdge,
  HUDMode,
  HUDUpdateNotice,
} from "./model";
export { defaultHUDConfig, HUD_METRICS } from "./model";
export type { HUDDragPreview, HUDDragStartArgs } from "./scene/useHUDDrag";
export type { HUDNativePointer } from "./scene/useHUDInteraction";
export type { HUDFlight, HUDFlightCommand } from "./scene/FlightLayer";

export interface HUDSceneProps {
  snapshot: Snapshot;
  config: HUDConfig;
  onConfigure?: () => void;
  onStats?: (target?: StatsTarget) => void;
  /** The product's newer version, and the way to it (the panel footer's arrow). */
  update?: HUDUpdateNotice | null;
  onUpdate?: () => void;
  /** The native host owns movement, edge snapping and configuration commits. */
  onDragStart?: (args: HUDDragStartArgs) => void | Promise<void>;
  /** Incremented by the host after its global mouse-button owner ends a drag. */
  dragEndSequence?: number;
  /** Transient native placement; it never changes the saved source configuration. */
  dragPreview?: HUDDragPreview | null;
  nativePointer?: HUDNativePointer;
  /** Whether the global pointer currently resides on this HUD's screen. */
  underPointer?: boolean;
  /** Whether an editable text field inside this HUD holds keyboard focus. */
  textFocused?: boolean;
  botDirections?: Record<string, Direction>;
  onBotLooked?: (keys: string[] | null) => void;
  glowHidden?: boolean;
  /** Host-measured camera housing in this viewport's logical coordinates. */
  notchFrame?: HUDRect;
  onRequestDecision?: (
    id: string,
    decision: HUDDecision,
  ) => void | Promise<void>;
  className?: string;
  height?: number;
  /** Parent-owned scenery, such as a desktop wallpaper. It never receives pointer events. */
  renderBackdrop?: ReactNode;
  /** A host's order to open or close the usage panel, as a demo script's
   * panel step gives (Open DemoScriptRunner: island forceOpen/forceCollapse).
   * Each new `sequence` is one order. */
  panelCommand?: { open: boolean; sequence: number };
  /** The host's recent flights between two agents' marks, oldest first. */
  flights?: readonly HUDFlightCommand[];
}

const position = (rect: HUDRect): CSSProperties => ({
  position: "absolute",
  left: rect.x,
  top: rect.y,
  width: rect.width,
  height: rect.height,
});

function rectContains(rect: HUDRect, point: { x: number; y: number }) {
  return (
    point.x >= rect.x &&
    point.x <= rect.x + rect.width &&
    point.y >= rect.y &&
    point.y <= rect.y + rect.height
  );
}

/** `value` while `live`; otherwise the value it had when it was last live. */
function useLatestWhile<T>(value: T, live: boolean): T {
  const latest = useRef(value);
  if (live) latest.current = value;
  return latest.current;
}

/** Draws again only when `version` changes; between, React keeps what it drew. */
const Redraw = memo(
  function Redraw({ draw }: { version: unknown; draw: () => ReactNode }) {
    return <>{draw()}</>;
  },
  (previous, next) => previous.version === next.version,
);

/**
 * A DOM renderer of one screen's HUD. The host owns native windows and their screen coordinates.
 * Geometry, hover timing, logo identity, three modes and the approval surface follow the Swift source.
 * See docs/parity-audit.md for the remaining native and data-model differences.
 */
export function HUDScene({
  snapshot,
  config: supplied,
  onConfigure,
  onStats,
  update,
  onUpdate,
  onDragStart,
  dragEndSequence = 0,
  dragPreview,
  nativePointer,
  underPointer = true,
  textFocused = false,
  botDirections,
  onBotLooked,
  glowHidden = false,
  notchFrame,
  onRequestDecision,
  className = "",
  height = 440,
  renderBackdrop,
  panelCommand,
  flights,
}: HUDSceneProps) {
  const sourceConfig = useMemo(
    () => ({ ...defaultHUDConfig, ...supplied }),
    [supplied],
  );
  const config = useMemo(
    () => ({ ...sourceConfig, ...dragPreview }),
    [sourceConfig, dragPreview],
  );
  const reduced = useReducedMotion();
  const reduceMotion = Boolean(
    reduced || !config.animations || !snapshot.settings.animations,
  );
  const id = useId().replaceAll(":", "");
  const scene = useRef<HTMLElement>(null);
  const dockMemory = useRef(new DockMemory());
  const content = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(720);
  const [contentHeight, setContentHeight] = useState<number>(
    HUD_METRICS.defaultPanelHeight,
  );
  const [systemLight, setSystemLight] = useState(false);
  const [hovered, setHovered] = useState<string | null>(null);
  const [pinned, setPinned] = useState(false);
  const [pointerInside, setPointerInside] = useState(false);
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
  const typing = Boolean(textFocused || domTyping);
  const suppressHover = useRef(false);
  const hover = useHoverMachine(config);
  const panelServices = usePanelServices();
  const heldUntil = panelServices?.draft?.heldUntil ?? 0;
  const [holdActive, setHoldActive] = useState(false);
  const vendors = useMemo(
    () => configuredVendors(snapshot, config),
    [snapshot, config.mode, config.panelLayout.order, config.panelLayout.hud],
  );
  // An agent's edit hold ends when the person takes over (Open
  // ScreenHUD+Hold): Escape, the pointer coming and going, Settings or
  // Statistics, or a drag. The panel then follows the pointer again.
  const enteredDuringHold = useRef(false);
  const endHold = useCallback(() => {
    if (!holdActive) return;
    setHoldActive(false);
    enteredDuringHold.current = false;
    panelServices?.draft?.release();
    hover.settle();
  }, [holdActive, panelServices, hover.settle]);
  const held = useRef(false);
  useEffect(() => {
    const remaining = heldUntil - Date.now();
    const wasHeld = held.current;
    held.current = remaining > 0;
    setHoldActive(remaining > 0);
    enteredDuringHold.current = false;
    if (remaining <= 0) {
      // Only a hold's end hands the panel back to the pointer. Running on
      // every new snapshot, this closed a panel opened any other way (a
      // demo's panel step, the keyboard) within a second.
      if (wasHeld) hover.settle();
      return;
    }
    if (
      config.mode !== "notch" &&
      config.separatesAgents &&
      !hovered &&
      vendors.length > 0
    ) {
      setHovered(vendors[0].key);
      setPinned(true);
    }
    hover.forceOpen();
    const timer = setTimeout(() => {
      held.current = false;
      setHoldActive(false);
      hover.settle();
    }, remaining);
    return () => clearTimeout(timer);
  }, [heldUntil]);
  const { state, forceOpen, collapse } = hover;
  useEffect(() => {
    if (!panelCommand) return;
    if (!panelCommand.open) {
      setPinned(false);
      collapse();
      return;
    }
    // A docked HUD that separates agents opens on its first agent, as a hold does.
    if (
      config.mode !== "notch" &&
      config.separatesAgents &&
      vendors.length > 0
    ) {
      setHovered(vendors[0].key);
      setPinned(true);
    }
    forceOpen();
  }, [panelCommand?.sequence]);
  const open = hover.open || holdActive;
  const enter = useCallback(() => {
    if (holdActive) enteredDuringHold.current = true;
    hover.enter();
  }, [holdActive, hover.enter]);
  const leave = useCallback(() => {
    if (!holdActive) hover.leave();
    else if (enteredDuringHold.current) {
      hover.inside.current = false;
      endHold();
    } else hover.inside.current = false;
  }, [holdActive, hover.leave, endHold]);

  const safeSceneHeight = Math.max(180, height);
  const screen = { width, height: safeSceneHeight };
  const {
    dragVisual,
    dragging,
    dragCandidate,
    suppressDragClick,
    considerDrag,
  } = useHUDDrag({
    snapshot,
    sourceConfig,
    notchFrame,
    screen,
    scene,
    onDragStart: onDragStart
      ? (args: HUDDragStartArgs) => {
          endHold();
          return onDragStart(args);
        }
      : undefined,
    dragPreview,
    dragEndSequence,
    open,
    forceOpen,
    leave,
    collapse,
    setPointerInside,
    setPinned,
    setHovered,
    suppressHover,
  });

  useEffect(() => {
    if (open)
      onBotLooked?.(config.separatesAgents && hovered ? [hovered] : null);
  }, [open, hovered, config.separatesAgents]);

  // Paused detection draws the still grey rim, as no readings do.
  const detectionPaused = Boolean(snapshot.status?.paused);
  const glowColors = useMemo(
    () =>
      detectionPaused
        ? []
        : vendors
            .filter((vendor) => vendor.used != null)
            .map((vendor) => statusColor(vendor.level)),
    [vendors, detectionPaused],
  );
  const metrics = useMemo(
    () => stripMetrics(config, vendors, snapshot.settings.quotaDisplay),
    [
      vendors,
      snapshot.settings.quotaDisplay,
      config.mode,
      config.edge,
      config.logoSize,
      config.logoEdgeDistance,
      config.gapScale,
      config.simpleSize,
      config.simpleGapScale,
      config.ringWidth,
      config.showsNumbers,
      config.simpleStyle,
      config.panelLayout,
      config.notchWidth,
      config.notchHeight,
      config.hasNotch,
    ],
  );
  const requests = [...snapshot.approvals].sort(
    (a, b) => Date.parse(a.createdAt) - Date.parse(b.createdAt),
  );
  const usagePanelOpenRef = useRef(false);
  const allowsInlineUsageAlert = !(
    config.mode !== "notch" && config.separatesAgents
  );
  const news = useIslandNews(
    snapshot,
    pointerInside || typing,
    requests,
    vendors.map((v) => v.key),
    glowHidden || Boolean(snapshot.status?.paused),
    underPointer,
    open && allowsInlineUsageAlert,
  );
  const event = news.event;
  const request = news.request;
  // The rim flashes once for each island alert, over its sender's colours.
  const glowFlash = useMemo(
    () =>
      glowAlert(
        request
          ? {
              id: request.id,
              kind: "request",
              vendor: vendorKey(request.source),
            }
          : event
            ? {
                id: event.id,
                kind: event.kind,
                vendor: vendorKey(event.source),
              }
            : null,
        vendors
          .filter((vendor) => vendor.used != null)
          .map((vendor) => vendor.key),
      ),
    [request?.id, event?.id, vendors],
  );
  useEffect(() => {
    if (open && !request && !event) {
      usagePanelOpenRef.current = true;
    } else if (!open) {
      usagePanelOpenRef.current = false;
    }
  }, [open, request, event]);
  const inUsagePanel = Boolean(
    open &&
    !request &&
    event &&
    allowsInlineUsageAlert &&
    (news.inUsagePanel || usagePanelOpenRef.current),
  );

  const {
    edge,
    strip,
    grab,
    light,
    fill,
    outline,
    shadow,
    sender,
    focusedVendor,
    delivery,
    detached,
    peek,
    scale,
    simplePeek,
    peekTuck,
    targetSize,
    surfaceFlare,
    flared,
    frame,
    cardVisible,
    notchGeometry,
    bodyPadding,
    surfacePadding,
    marginStyle,
    measurementWidth,
  } = useHUDLayout({
    config,
    snapshot,
    vendors,
    metrics,
    screen,
    notchFrame,
    contentHeight,
    systemLight,
    open,
    pinned,
    dragging,
    reduceMotion,
    hovered,
    request,
    event,
    inUsagePanel,
    requestsCount: requests.length,
  });

  const [heldFrame, setHeldFrame] = useState<HUDRect | null>(null);
  const lastFrameRef = useRef<HUDRect | null>(null);
  const lastPointerPointRef = useRef<{ x: number; y: number } | null>(null);
  useLayoutEffect(() => {
    if (!cardVisible || !pointerInside || dragging) {
      lastFrameRef.current = cardVisible ? frame : null;
      setHeldFrame((current) => (current ? null : current));
      return;
    }
    const prev = heldFrame ?? lastFrameRef.current;
    lastFrameRef.current = frame;
    if (!prev) return;
    const minX = Math.min(prev.x, frame.x);
    const minY = Math.min(prev.y, frame.y);
    const maxX = Math.max(prev.x + prev.width, frame.x + frame.width);
    const maxY = Math.max(prev.y + prev.height, frame.y + frame.height);
    const shrunk =
      maxX - minX > frame.width + 1 || maxY - minY > frame.height + 1;
    if (!shrunk) {
      if (heldFrame) setHeldFrame(null);
      return;
    }
    const pt = nativePointer?.point ?? lastPointerPointRef.current;
    if (pt && (rectContains(frame, pt) || rectContains(strip, pt))) {
      if (heldFrame) setHeldFrame(null);
      return;
    }
    if (
      !heldFrame ||
      Math.abs(heldFrame.x - minX) > 0.5 ||
      Math.abs(heldFrame.y - minY) > 0.5 ||
      Math.abs(heldFrame.width - (maxX - minX)) > 0.5 ||
      Math.abs(heldFrame.height - (maxY - minY)) > 0.5
    ) {
      setHeldFrame({
        x: minX,
        y: minY,
        width: maxX - minX,
        height: maxY - minY,
      });
    }
  }, [
    cardVisible,
    pointerInside,
    dragging,
    frame.x,
    frame.y,
    frame.width,
    frame.height,
    strip.x,
    strip.y,
    strip.width,
    strip.height,
    nativePointer?.point?.x,
    nativePointer?.point?.y,
    heldFrame,
  ]);

  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: light)");
    const change = () => setSystemLight(media.matches);
    change();
    media.addEventListener("change", change);
    return () => media.removeEventListener("change", change);
  }, []);
  useLayoutEffect(() => {
    if (!scene.current) return;
    setWidth(Math.max(360, scene.current.getBoundingClientRect().width));
    const observer = new ResizeObserver((entries) =>
      setWidth(Math.max(360, entries[0].contentRect.width)),
    );
    observer.observe(scene.current);
    return () => observer.disconnect();
  }, []);
  useLayoutEffect(() => {
    const element = content.current;
    if (!element) return;
    const measure = (height: number) => {
      const naturalHeight = Math.ceil(height);
      if (naturalHeight > 0) setContentHeight(naturalHeight);
    };
    // Measure the same real content before the first hover, as the native hosting view does.
    measure(element.getBoundingClientRect().height);
    const observer = new ResizeObserver((entries) =>
      measure(
        entries[0].borderBoxSize?.[0]?.blockSize ??
          element.getBoundingClientRect().height,
      ),
    );
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const {
    vendorUnderPointer,
    followMark,
    entered,
    domExited,
    pin,
    handoff,
    configurePanel,
    showPanelStats,
  } = useHUDInteraction({
    config,
    edge,
    strip,
    metrics,
    vendors,
    width,
    height,
    scene,
    open,
    hoverPhase: state.phase,
    hovered,
    pointerInside,
    dragVisual,
    dragPreview,
    dragCandidate,
    suppressDragClick,
    suppressHover,
    hoverInside: hover.inside,
    enter,
    leave,
    forceOpen,
    collapse,
    setHovered,
    setPinned,
    setPointerInside,
    request,
    event,
    inUsagePanel,
    typing,
    nativePointer,
    onConfigure,
    onStats,
    onEscape: endHold,
  });

  // A card's Open Agent plugins hands over as the settings button does.
  const openSettingsPage = panelServices?.openSettingsPage;
  const openSettings = useMemo(
    () =>
      openSettingsPage
        ? (page: string) => {
            endHold();
            handoff(() => openSettingsPage(page));
          }
        : undefined,
    [openSettingsPage, endHold, handoff],
  );
  const eventContent =
    request || event ? (
      <>
        {request ? (
          <ApprovalContent
            requests={requests}
            selected={request}
            select={news.select}
            decide={onRequestDecision}
            openSettings={openSettings}
            light={light}
            language={snapshotLanguage(snapshot)}
          />
        ) : event ? (
          <NewsContent
            event={event}
            snapshot={snapshot}
            dismiss={news.dismiss}
            onStats={showPanelStats}
            handoff={handoff}
            light={light}
            botLogos={config.botLogos}
          />
        ) : null}
        <PendingEventRows
          queue={news.queue}
          currentID={(request ?? event)!.id}
          requests={requests}
          select={news.select}
          includeRequests={!request}
          light={light}
          zh={snapshotLanguage(snapshot) === "zh"}
        />
      </>
    ) : null;

  // The panel's session rows hand over as Open's ScreenHUD does: the HUD
  // collapses first, then the Statistics window opens.
  const openStatsAt = useCallback(
    (target: StatsTarget) => {
      if (!onStats) return;
      endHold();
      handoff(() => onStats(target));
    },
    [onStats, endHold, handoff],
  );
  const listedSessions = useMemo<ListedSessions>(
    () => ({ openStats: openStatsAt }),
    [openStatsAt],
  );
  // The footer's Settings, Statistics and update arrow end an agent's edit
  // hold, then hand over: each opens another window.
  const footer = useMemo(
    () => ({
      configure:
        configurePanel &&
        (() => {
          endHold();
          configurePanel();
        }),
      stats:
        showPanelStats &&
        (() => {
          endHold();
          showPanelStats();
        }),
      update:
        onUpdate &&
        (() => {
          endHold();
          handoff(onUpdate);
        }),
    }),
    [configurePanel, showPanelStats, onUpdate, endHold, handoff],
  );
  // The panel's body, drawn by the visible panel and by the hidden copy
  // below, which measures its natural height (no `maxHeight`) over the
  // snapshot it last drew.
  const panelBody = (shown: Snapshot, maxHeight?: number) => (
    <>
      {eventContent && !inUsagePanel ? (
        eventContent
      ) : (
        <SourceHoverPanel
          active={open}
          config={config}
          snapshot={shown}
          focusVendor={focusedVendor?.name}
          light={light}
          Logo={HUDLogo}
          onConfigure={footer.configure}
          onStats={footer.stats}
          update={update}
          onUpdate={footer.update}
          inlineAlert={
            inUsagePanel && event ? (
              <InlineEventRow
                event={event}
                snapshot={shown}
                light={light}
                botLogos={config.botLogos}
                zh={snapshotLanguage(shown) === "zh"}
                onOpen={() => {
                  news.dismiss();
                  footer.stats?.();
                }}
              />
            ) : undefined
          }
          maxHeight={maxHeight}
        />
      )}
      <RequestDeliveryNotice
        delivery={delivery}
        light={light}
        language={snapshotLanguage(shown)}
      />
    </>
  );
  // The hidden copy measures the panel for its next opening, as the native
  // hosting view measured its fitting size. A pointer move never draws it
  // again, and at rest (collapsed, no card out, the pointer away) neither does
  // a data update: it keeps the snapshot it last drew and catches up as the
  // pointer arrives, the panel opens or a card comes out.
  const measuring =
    open ||
    state.phase !== "collapsed" ||
    pointerInside ||
    Boolean(request || event);
  const measuredSnapshot = useLatestWhile(snapshot, measuring);
  const measuredServices = useLatestWhile(panelServices, measuring);
  const configKey = useMemo(() => JSON.stringify(config), [config]);
  // What the copy draws, taken when something it shows changes.
  const measured = useMemo(
    () => ({
      snapshot: measuredSnapshot,
      services: measuredServices,
      listed: listedSessions,
    }),
    [
      measuredSnapshot,
      measuredServices,
      configKey,
      light,
      open,
      focusedVendor?.name,
      request,
      event,
      news.queue,
      inUsagePanel,
      delivery,
    ],
  );
  return (
    <HUDGroundProvider look={config}>
      <ListedSessionsContext.Provider value={listedSessions}>
        <section
          ref={scene}
          className={`hud-scene ${className}`}
          data-mode={config.mode}
          data-edge={edge}
          data-hover={state.phase}
          data-pinned={pinned}
          data-appearance={light ? "light" : "dark"}
          aria-label={`Agent HUD ${config.mode === "notch" ? "Classic" : config.mode === "logos" ? "Dynamic" : "Simple"} preview`}
          style={{
            position: "relative",
            height: safeSceneHeight,
            width: "100%",
            overflow: "hidden",
            isolation: "isolate",
            fontFamily:
              '-apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif',
            color: light ? "#1c1c1e" : "#f2f2f7",
            cursor: dragVisual ? "grabbing" : undefined,
          }}
          onFocusCapture={checkDomTyping}
          onBlurCapture={() => queueMicrotask(checkDomTyping)}
          onPointerMoveCapture={(event) => {
            if (nativePointer) return;
            const bounds = scene.current?.getBoundingClientRect();
            if (!bounds) return;
            const pt = {
              x: event.clientX - bounds.left,
              y: event.clientY - bounds.top,
            };
            lastPointerPointRef.current = pt;
            if (
              heldFrame &&
              (rectContains(frame, pt) || rectContains(strip, pt))
            ) {
              setHeldFrame(null);
            }
          }}
          onPointerLeave={domExited}
        >
          <div
            ref={content}
            aria-hidden="true"
            inert
            data-hud-measurement
            style={{
              position: "absolute",
              left: 0,
              top: 0,
              width: measurementWidth,
              visibility: "hidden",
              pointerEvents: "none",
              paddingTop: bodyPadding.top,
              paddingRight: bodyPadding.right,
              paddingBottom: bodyPadding.bottom,
              paddingLeft: bodyPadding.left,
            }}
          >
            <Redraw
              version={measured}
              draw={() => {
                const drawn = (
                  <ListedSessionsContext.Provider value={measured.listed}>
                    {panelBody(measured.snapshot)}
                  </ListedSessionsContext.Provider>
                );
                return measured.services ? (
                  <PanelServicesProvider value={measured.services}>
                    {drawn}
                  </PanelServicesProvider>
                ) : (
                  drawn
                );
              }}
            />
          </div>
          {renderBackdrop && (
            <div
              style={{
                position: "absolute",
                inset: 0,
                pointerEvents: "none",
                zIndex: -2,
              }}
            >
              {renderBackdrop}
            </div>
          )}
          {config.mode === "notch" && !glowHidden && (
            <NotchGlow
              geometry={notchGeometry}
              config={config}
              vendors={vendors}
              colors={glowColors}
              reduceMotion={reduceMotion}
              alert={glowFlash}
            />
          )}
          {heldFrame && cardVisible && pointerInside && !dragging && (
            <div
              className="hud-hover-floor"
              aria-hidden="true"
              onPointerEnter={(e) => {
                if (!nativePointer) entered(e.altKey);
              }}
              onPointerLeave={domExited}
              style={{
                ...position(heldFrame),
                zIndex: 1,
                background: "transparent",
              }}
            />
          )}
          <HUDExpandedSurface
            config={config}
            edge={edge}
            cardVisible={cardVisible}
            detached={detached}
            peek={peek}
            simplePeek={simplePeek}
            open={open}
            reduceMotion={reduceMotion}
            dragging={dragging}
            frame={frame}
            strip={strip}
            flared={flared}
            surfaceFlare={surfaceFlare}
            metricsRadius={metrics.radius}
            targetSize={targetSize}
            notchGeometry={notchGeometry}
            light={light}
            fill={fill}
            outline={outline}
            shadow={shadow}
            nativePointer={nativePointer}
            entered={entered}
            domExited={domExited}
            focusedVendor={focusedVendor}
            scale={scale}
            peekTuck={peekTuck}
            pin={pin}
            request={request}
            event={event}
            requests={requests}
            snapshot={snapshot}
            forceOpen={forceOpen}
            dismissNews={news.dismiss}
            showPanelStats={showPanelStats}
            surfacePadding={surfacePadding}
            inUsagePanel={inUsagePanel}
          >
            {panelBody(
              snapshot,
              Math.max(
                40,
                frame.height - surfacePadding.top - surfacePadding.bottom,
              ),
            )}
          </HUDExpandedSurface>
          <div
            className="hud-resting-strip"
            style={{
              ...position(strip),
              zIndex: 3,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              userSelect: "none",
              touchAction: onDragStart ? "none" : undefined,
              cursor: dragVisual ? "grabbing" : undefined,
            }}
            onPointerDownCapture={considerDrag}
            onClickCapture={(event) => {
              if (!suppressDragClick.current || event.detail === 0) return;
              event.preventDefault();
              event.stopPropagation();
              suppressDragClick.current = false;
            }}
            onPointerEnter={(e) => {
              if (!nativePointer) {
                followMark(e);
                entered(e.altKey);
              }
            }}
            onPointerLeave={domExited}
            onPointerMove={(event) => {
              if (nativePointer) return;
              followMark(event);
              if (
                config.openImmediatelyAtTop &&
                edge === "top" &&
                (state.phase === "opening" || state.phase === "collapsed") &&
                !suppressHover.current &&
                (!config.requiresOptionToOpen ||
                  event.altKey ||
                  hover.inside.current)
              ) {
                const bounds = scene.current?.getBoundingClientRect();
                const x = event.clientX - (bounds?.left ?? 0);
                const y = event.clientY - (bounds?.top ?? 0);
                if (x >= strip.x && x <= strip.x + strip.width && y <= 1)
                  forceOpen();
              }
            }}
          >
            {onDragStart && (
              <div
                className="hud-grab-surface"
                aria-hidden="true"
                // The resting strip owns drag/click capture and hover for this child.
                style={{
                  position: "absolute",
                  left: grab.frame.x - strip.x,
                  top: grab.frame.y - strip.y,
                  width: grab.frame.width,
                  height: grab.frame.height,
                  zIndex: 4,
                  background: "transparent",
                  cursor: dragVisual ? "grabbing" : "default",
                }}
                onClick={(event) => {
                  const key = vendorUnderPointer(event) ?? hovered;
                  if (key) pin(key);
                }}
              />
            )}
            <HUDMarkStrip
              id={id}
              config={config}
              edge={edge}
              strip={strip}
              metrics={metrics}
              vendors={vendors}
              glowColors={glowColors}
              glowAlert={glowFlash}
              glowHidden={glowHidden}
              cardVisible={cardVisible}
              detached={detached}
              light={light}
              fill={fill}
              outline={outline}
              marginStyle={marginStyle}
              dragVisual={dragVisual}
              hovered={hovered}
              pointerInside={pointerInside}
              open={open}
              sender={sender}
              focusedVendor={focusedVendor}
              botDirections={botDirections}
              dockMemory={dockMemory}
              nativePointer={nativePointer}
              reduceMotion={reduceMotion}
              hasAlert={Boolean(request || event)}
              suppressHover={suppressHover}
              setHovered={setHovered}
              forceOpen={forceOpen}
              pin={pin}
            />
          </div>
          {dragVisual && (
            <svg
              aria-hidden="true"
              className="hud-drag-outline"
              width={grab.frame.width}
              height={grab.frame.height}
              style={{
                ...position(grab.frame),
                zIndex: 5,
                overflow: "visible",
                pointerEvents: "none",
                filter: "drop-shadow(0 0 2px rgba(0,0,0,0.8))",
              }}
            >
              <rect
                x={grab.outline.x - grab.frame.x}
                y={grab.outline.y - grab.frame.y}
                width={grab.outline.width}
                height={grab.outline.height}
                rx={grab.radius}
                fill="rgba(255,255,255,0.025)"
                stroke="rgba(255,255,255,0.9)"
                strokeWidth={1.25}
                strokeDasharray="4 3"
              />
            </svg>
          )}
          <FlightLayer
            flights={flights}
            mode={config.mode}
            edge={edge}
            strip={strip}
            scene={scene}
            botLogos={config.botLogos}
            reduceMotion={reduceMotion}
            light={light}
          />
          <span id={`hud-instructions-${id}`} className="sr-only">
            {snapshotLanguage(snapshot) === "zh"
              ? "悬停展开。在分开显示的模式下，点按一个智能体打开它的完整面板。"
              : "Hover to expand. In separated agent mode, click an agent to open its full panel."}
          </span>
        </section>
      </ListedSessionsContext.Provider>
    </HUDGroundProvider>
  );
}

export default HUDScene;
