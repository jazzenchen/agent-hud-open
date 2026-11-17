import { useRef, useState, type PointerEvent } from "react";
import {
  boxFrom,
  INK,
  pinNumbers,
  pinRadius,
  strokeFor,
  type Mark,
  type MarkTool,
} from "./mark-model";

/** The marks over the picture, drawn in its own pixels (`size`), and the
 * pointer that adds one with `tool`: a box dragged, a line drawn, a pin put
 * down with a click. */
export function MarkLayer({
  size,
  marks,
  tool,
  add,
}: {
  size: { w: number; h: number };
  marks: Mark[];
  tool: MarkTool;
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
      tool === "box" ? boxFrom(point, point) : { kind: "pen", points: [point] },
    );
  };
  const move = (event: PointerEvent) => {
    if (!start.current || !drawing) return;
    const point = at(event);
    setDrawing(
      drawing.kind === "box"
        ? boxFrom(start.current, point)
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
    // A click with the box or the pen leaves nothing.
    if (done.kind === "box" && done.w * done.h < 16) return;
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
            <circle cx={mark.x} cy={mark.y} r={radius} fill={INK} />
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
