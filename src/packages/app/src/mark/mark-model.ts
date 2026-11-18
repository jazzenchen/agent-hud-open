// What the markup window draws on a picture, in the picture's own pixels,
// and the marked picture it sends back.

export type MarkTool = "box" | "circle" | "pen" | "pin";

/** A box, or the circle drawn inside one. */
export type Frame = {
  kind: "box" | "circle";
  x: number;
  y: number;
  w: number;
  h: number;
};

export type Mark =
  | Frame
  | { kind: "pen"; points: [number, number][] }
  | { kind: "pin"; x: number; y: number; text: string };

/** The marks' one colour: a red that reads on light and dark pictures. */
export const INK = "#FF3B30";

/** A line as thick as a picture this wide needs to be seen. */
export const strokeFor = (width: number) =>
  Math.max(3, Math.round(width / 320));

/** The white edge round a pin, so it reads on red and dark pictures too. */
export const RING = "#FFFFFF";
export const ringFor = (radius: number) => Math.max(2, radius * 0.16);

/** A pin's radius on a picture this wide. */
export const pinRadius = (width: number) =>
  Math.max(13, Math.round(width / 70));

/** Each pin's number, counting pins only, in the order they were put down. */
export function pinNumbers(marks: Mark[]): Map<number, number> {
  const numbers = new Map<number, number>();
  marks.forEach((mark, index) => {
    if (mark.kind === "pin") numbers.set(index, numbers.size + 1);
  });
  return numbers;
}

/** A box or a circle from two corners, whichever way it was dragged. */
export function frameFrom(
  kind: Frame["kind"],
  from: [number, number],
  to: [number, number],
): Frame {
  return {
    kind,
    x: Math.min(from[0], to[0]),
    y: Math.min(from[1], to[1]),
    w: Math.abs(to[0] - from[0]),
    h: Math.abs(to[1] - from[1]),
  };
}

function loaded(url: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.onload = () => resolve(image);
    image.onerror = () => reject(new Error("The picture did not load"));
    image.src = url;
  });
}

/** The picture at `url` with `marks` drawn in, as PNG base64. */
export async function markedPicture(url: string, marks: Mark[]) {
  const image = await loaded(url);
  const width = image.naturalWidth;
  const height = image.naturalHeight;
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext("2d");
  if (!context) throw new Error("The picture could not be drawn");
  context.drawImage(image, 0, 0);
  context.strokeStyle = INK;
  context.lineWidth = strokeFor(width);
  context.lineJoin = "round";
  context.lineCap = "round";
  const numbers = pinNumbers(marks);
  const radius = pinRadius(width);
  marks.forEach((mark, index) => {
    if (mark.kind === "box") {
      context.strokeRect(mark.x, mark.y, mark.w, mark.h);
    } else if (mark.kind === "circle") {
      context.beginPath();
      context.ellipse(
        mark.x + mark.w / 2,
        mark.y + mark.h / 2,
        mark.w / 2,
        mark.h / 2,
        0,
        0,
        Math.PI * 2,
      );
      context.stroke();
    } else if (mark.kind === "pen") {
      context.beginPath();
      mark.points.forEach(([x, y], at) =>
        at ? context.lineTo(x, y) : context.moveTo(x, y),
      );
      context.stroke();
    } else {
      context.beginPath();
      context.arc(mark.x, mark.y, radius, 0, Math.PI * 2);
      context.fillStyle = INK;
      context.fill();
      context.strokeStyle = RING;
      context.lineWidth = ringFor(radius);
      context.stroke();
      context.strokeStyle = INK;
      context.lineWidth = strokeFor(width);
      context.fillStyle = "#FFFFFF";
      context.font = `600 ${Math.round(radius * 1.15)}px -apple-system, sans-serif`;
      context.textAlign = "center";
      context.textBaseline = "middle";
      context.fillText(String(numbers.get(index)), mark.x, mark.y + 1);
    }
  });
  return canvas.toDataURL("image/png").split(",")[1];
}
