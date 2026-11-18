import type {
  MutableRefObject,
  PointerEvent as ReactPointerEvent,
  ReactNode,
} from "react";
import { position, type HUDRect } from "../geometry";
import type { dragSurfaceGeometry } from "../placement";
import type { HUDNativePointer } from "./useHUDInteraction";

/** The resting strip the marks sit in: it takes the pointer for them, with
 * the surface a drag grabs it by and, while it is dragged, its outline. */
export function RestingStrip({
  strip,
  grab,
  draggable,
  dragVisual,
  retracted = false,
  considerDrag,
  suppressDragClick,
  nativePointer,
  followMark,
  entered,
  domExited,
  openAtTop,
  vendorUnderPointer,
  hovered,
  pin,
  children,
}: {
  strip: HUDRect;
  grab: ReturnType<typeof dragSurfaceGeometry>;
  /** The host moves the HUD: the strip starts a drag. */
  draggable: boolean;
  dragVisual: boolean;
  retracted?: boolean;
  considerDrag: (event: ReactPointerEvent<HTMLDivElement>) => void;
  suppressDragClick: MutableRefObject<boolean>;
  nativePointer?: HUDNativePointer;
  followMark: (event: { clientX: number; clientY: number }) => void;
  entered: (optionDown?: boolean) => void;
  domExited: (event?: { relatedTarget?: EventTarget | null }) => void;
  openAtTop: (event: {
    clientX: number;
    clientY: number;
    altKey: boolean | null;
  }) => void;
  vendorUnderPointer: (event: {
    clientX: number;
    clientY: number;
  }) => string | null;
  hovered: string | null;
  pin: (key?: string) => void;
  /** The marks. */
  children: ReactNode;
}) {
  const grabBox = retracted
    ? { left: 0, top: 0, width: strip.width, height: strip.height }
    : {
        left: grab.frame.x - strip.x,
        top: grab.frame.y - strip.y,
        width: grab.frame.width,
        height: grab.frame.height,
      };
  return (
    <>
      <div
        className="hud-resting-strip"
        style={{
          ...position(strip),
          zIndex: 3,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          userSelect: "none",
          touchAction: draggable ? "none" : undefined,
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
          openAtTop(event);
        }}
      >
        {draggable && (
          <div
            className="hud-grab-surface"
            aria-hidden="true"
            // The resting strip owns drag/click capture and hover for this child.
            style={{
              position: "absolute",
              ...grabBox,
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
        {children}
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
    </>
  );
}
