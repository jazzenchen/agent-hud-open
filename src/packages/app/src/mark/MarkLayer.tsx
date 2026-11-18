import { useRef, useState, type PointerEvent } from "react";
import {
  frameFrom,
  INK,
  pinNumbers,
  pinRadius,
  ringFor,
  RING,
  strokeFor,
  type Mark,
  type MarkTool,
} from "./mark-model";

/** The marks over the picture, drawn in its own pixels (`size`), and the
 * pointer that adds one with `tool`: a box or a circle dragged, a line
 * drawn, a pin put down with a click. The lit pin has a halo. */
export function MarkLayer({
  size,
  marks,
  tool,
  lit,
  add,
}: {
  size: { w: number; h: number };
  marks: Mark[];
  tool: MarkTool;
  lit: number | null;
  add: (mark: Mark) => void;
}) {
  const svg = useRef<SVGSVGElement>(null);
  const start = useRef<[number, number] | null>(null);
  const [drawing, setDrawing] = useState<Mark | null>(null);
  const at = (event: PointerEvent): [number, number] => {
    const rect = svg.current!.getBoundingClientRect();
    return [
      ((event.clientX - rect.left) / rect.width) * size.w,
      ((event.clientY - rect.top) / rect.height) * size.h,
    ];
  };
  const down = (event: PointerEvent<SVGSVGElement>) => {
    const point = at(event);
    if (tool === "pin") {
      // The click must not take the keys back from the new comment's field.
      event.preventDefault();
      add({ kind: "pin", x: point[0], y: point[1], text: "" });
      return;
    }
    event.currentTarget.setPointerCapture(event.pointerId);
    start.current = point;
    setDrawing(
      tool === "box" || tool === "circle"
        ? frameFrom(tool, point, point)
        : { kind: "pen", points: [point] },
    );
  };
  const move = (event: PointerEvent) => {
    if (!start.current || !drawing) return;
    const point = at(event);
    setDrawing(
      drawing.kind === "box" || drawing.kind === "circle"
        ? frameFrom(drawing.kind, start.current, point)
        : drawing.kind === "pen"
          ? { kind: "pen", points: [...drawing.points, point] }
          : drawing,
    );
  };
  const up = () => {
    const done = drawing;
    start.current = null;
    setDrawing(null);
    if (!done) return;
    // A click with the box, the circle or the pen leaves nothing.
    if ((done.kind === "box" || done.kind === "circle") && done.w * done.h < 16)
      return;
    if (done.kind === "pen" && done.points.length < 2) return;
    add(done);
  };
  const stroke = strokeFor(size.w);
  const radius = pinRadius(size.w);
  const numbers = pinNumbers(marks);
  const shapes = drawing ? [...marks, drawing] : marks;
  return (
    <svg
      ref={svg}
      viewBox={`0 0 ${size.w} ${size.h}`}
      preserveAspectRatio="none"
      onPointerDown={down}
      onPointerMove={move}
      onPointerUp={up}
      onPointerCancel={up}
      style={{
        position: "absolute",
        inset: 0,
        width: "100%",
        height: "100%",
        cursor: tool === "pin" ? "copy" : "crosshair",
        touchAction: "none",
      }}
    >
      {shapes.map((mark, index) =>
        mark.kind === "box" ? (
          <rect
            key={index}
            x={mark.x}
            y={mark.y}
            width={mark.w}
            height={mark.h}
            fill="none"
            stroke={INK}
            strokeWidth={stroke}
          />
        ) : mark.kind === "circle" ? (
          <ellipse
            key={index}
            cx={mark.x + mark.w / 2}
            cy={mark.y + mark.h / 2}
            rx={mark.w / 2}
            ry={mark.h / 2}
            fill="none"
            stroke={INK}
            strokeWidth={stroke}
          />
        ) : mark.kind === "pen" ? (
          <polyline
            key={index}
            points={mark.points.map(([x, y]) => `${x},${y}`).join(" ")}
            fill="none"
            stroke={INK}
            strokeWidth={stroke}
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        ) : (
          <g key={index}>
            {lit === index && (
              <circle
                cx={mark.x}
                cy={mark.y}
                r={radius * 1.8}
                fill={INK}
                opacity={0.28}
              />
            )}
            <circle
              cx={mark.x}
              cy={mark.y}
              r={radius}
              fill={INK}
              stroke={RING}
              strokeWidth={ringFor(radius)}
            />
            <text
              x={mark.x}
              y={mark.y + 1}
              fill="#FFFFFF"
              fontSize={radius * 1.15}
              fontWeight={600}
              textAnchor="middle"
              dominantBaseline="middle"
            >
              {numbers.get(index)}
            </text>
          </g>
        ),
      )}
    </svg>
  );
}
