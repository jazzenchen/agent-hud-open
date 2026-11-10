import {
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { useReducedMotion } from "motion/react";
import type { Snapshot } from "@agent-hud/protocol";
export { HUDArtwork as HUDLogo } from "./HUDArtwork";
import { DockMemory, type Direction } from "./bots/mark-director";
import { AlertStack } from "./scene/AlertStack";
import { HUDExpandedSurface } from "./scene/HUDExpandedSurface";
import { HUDMarkStrip } from "./scene/HUDMarkStrip";
import { FlightLayer } from "./scene/FlightLayer";
import type { HUDFlightCommand } from "./scene/flight-queue";
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
import { useDomTyping } from "./scene/useDomTyping";
import { useEditHold } from "./scene/useEditHold";
import { useHeldFrame } from "./scene/useHeldFrame";
import { MeasuringCopy, useMeasured } from "./scene/MeasuringCopy";
import { DockEnds } from "./ends/DockEnds";
import { useDockEnds } from "./ends/useDockEnds";
import { PanelBody } from "./scene/PanelBody";
import { RestingStrip } from "./scene/RestingStrip";
import { HoverFloor, SceneBackdrop } from "./scene/SceneFloors";
import { usePanelHandoffs } from "./scene/usePanelHandoffs";
import { useSceneAlerts } from "./scene/useSceneAlerts";
import { useStripMetrics } from "./scene/useStripMetrics";
import { usePanelServices } from "./PanelContext";
import type { HUDRect } from "./geometry";
import { useHoverMachine } from "./hover";
import {
  defaultHUDConfig,
  HUD_METRICS,
  statusColor,
  configuredVendors,
  type HUDConfig,
  type HUDDecision,
  type HUDUpdateNotice,
} from "./model";
import { snapshotLanguage } from "@agent-hud/ui/format";
import {
  ListedSessionsContext,
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
export type { HUDFlight, HUDFlightCommand } from "./scene/flight-queue";

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
  const ends = useDockEnds(reduceMotion);
  const id = useId().replaceAll(":", "");
  const scene = useRef<HTMLElement>(null);
  const dockMemory = useRef(new DockMemory());
  const [width, setWidth] = useState(720);
  const [contentHeight, setContentHeight] = useState<number>(
    HUD_METRICS.defaultPanelHeight,
  );
  const [systemLight, setSystemLight] = useState(false);
  const [hovered, setHovered] = useState<string | null>(null);
  const [pinned, setPinned] = useState(false);
  const [pointerInside, setPointerInside] = useState(false);
  const { domTyping, checkDomTyping } = useDomTyping(scene);
  const typing = Boolean(textFocused || domTyping);
  const suppressHover = useRef(false);
  const hover = useHoverMachine(config);
  const panelServices = usePanelServices();
  const vendors = useMemo(
    () => configuredVendors(snapshot, config),
    [snapshot, config],
  );
  const { holdActive, endHold, enter, leave } = useEditHold({
    hover,
    panelServices,
    config,
    vendors,
    hovered,
    setHovered,
    setPinned,
  });
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
  const metrics = useStripMetrics(
    config,
    vendors,
    snapshot.settings.quotaDisplay,
  );
  const { requests, news, request, event, glowFlash, inUsagePanel, endCounts } =
    useSceneAlerts({
      snapshot,
      config,
      vendors,
      held: pointerInside || typing,
      silenced: glowHidden || Boolean(snapshot.status?.paused),
      underPointer,
      open,
      ends,
    });

  const {
    edge,
    strip,
    endRects,
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
    ends: { view: news.ends.view, counts: endCounts },
  });

  const { heldFrame, trackPointer } = useHeldFrame({
    cardVisible,
    pointerInside,
    dragging,
    frame,
    strip,
    nativePointer,
    scene,
  });

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

  const {
    vendorUnderPointer,
    followMark,
    entered,
    domExited,
    pin,
    openAtTop,
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
    ends: { showing: Boolean(news.ends.view), hovered: ends.hovered },
  });

  const { openSettings, listedSessions, footer } = usePanelHandoffs({
    endHold,
    handoff,
    openSettingsPage: panelServices?.openSettingsPage,
    onStats,
    configurePanel,
    showPanelStats,
    onUpdate,
  });
  const eventContent =
    request || event ? (
      <AlertStack
        requests={requests}
        request={request}
        event={event}
        news={news}
        decide={onRequestDecision}
        openSettings={openSettings}
        onStats={showPanelStats}
        handoff={handoff}
        light={light}
        snapshot={snapshot}
        botLogos={config.botLogos}
      />
    ) : null;

  // The panel's body, drawn by the visible panel and by the hidden copy
  // below, which measures its natural height (no `maxHeight`) over the
  // snapshot it last drew.
  const panelBody = (shown: Snapshot, maxHeight?: number) => (
    <PanelBody
      snapshot={shown}
      maxHeight={maxHeight}
      alertStack={eventContent}
      inUsagePanel={inUsagePanel}
      event={event}
      open={open}
      config={config}
      focusVendor={focusedVendor?.name}
      light={light}
      footer={footer}
      update={update}
      dismissNews={news.dismiss}
      delivery={delivery}
    />
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
  const configKey = useMemo(() => JSON.stringify(config), [config]);
  // What the copy draws, taken when something it shows changes.
  const measured = useMeasured(
    snapshot,
    panelServices,
    listedSessions,
    measuring,
    [
      configKey,
      light,
      open,
      focusedVendor?.name,
      request,
      event,
      news.queue,
      news.ends.view === "tail" ? news.ends.tail : news.ends.view,
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
          aria-label={`Agent HUD ${config.mode === "notch" ? "Classic" : "Simple"} preview`}
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
          onPointerMoveCapture={trackPointer}
          onPointerLeave={domExited}
        >
          <MeasuringCopy
            measured={measured}
            body={panelBody}
            width={measurementWidth}
            padding={bodyPadding}
            onHeight={setContentHeight}
          />
          <SceneBackdrop>{renderBackdrop}</SceneBackdrop>
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
          <HoverFloor
            frame={cardVisible && pointerInside && !dragging ? heldFrame : null}
            native={Boolean(nativePointer)}
            entered={entered}
            domExited={domExited}
          />
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
          <RestingStrip
            strip={strip}
            grab={grab}
            draggable={Boolean(onDragStart)}
            dragVisual={dragVisual}
            considerDrag={considerDrag}
            suppressDragClick={suppressDragClick}
            nativePointer={nativePointer}
            followMark={followMark}
            entered={entered}
            domExited={domExited}
            openAtTop={openAtTop}
            vendorUnderPointer={vendorUnderPointer}
            hovered={hovered}
            pin={pin}
          >
            <HUDMarkStrip
              id={id}
              config={config}
              edge={edge}
              strip={strip}
              metrics={metrics}
              vendors={vendors}
              cardVisible={cardVisible}
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
          </RestingStrip>
          <DockEnds
            ends={ends}
            geometry={endRects}
            counts={endCounts}
            fade={news.ends.fade}
            config={config}
            edge={edge}
            strip={strip}
            frame={frame}
            cardVisible={cardVisible}
            showing={Boolean(news.ends.view)}
            dragging={dragging}
            light={light}
            fill={fill}
            outline={outline}
            reduceMotion={reduceMotion}
            nativePointer={nativePointer}
            scene={scene}
            collapsed={state.phase === "collapsed"}
            pointerInside={pointerInside}
            entered={entered}
            domExited={domExited}
            forceOpen={forceOpen}
          />
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
