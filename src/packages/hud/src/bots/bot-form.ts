import { botSpecs, type BotSpec, type Point } from "./bot-spec";
import { colorField } from "./bot-field";
/** BotForm.swift: 360 radial samples and original 60px paint-field spread. */
function rounded(points: Point[], radius: number): Path2D {
  const path = new Path2D(),
    n = points.length;
  const middle = (a: Point, b: Point): Point => [
    (a[0] + b[0]) / 2,
    (a[1] + b[1]) / 2,
  ];
  const first = middle(points[n - 1], points[0]);
  path.moveTo(...first);
  points.forEach((point, index) => {
    const next = middle(point, points[(index + 1) % n]);
    path.arcTo(...point, ...next, radius);
    path.lineTo(...next);
  });
  path.closePath();
  return path;
}
function primitive(specification: BotSpec): Path2D {
  const s = specification,
    [cx, cy] = s.center,
    r = s.radius,
    shape = s.shape;
  const path = new Path2D();
  if (shape.kind === "circle" || shape.kind === "outline") {
    path.arc(cx, cy, r, 0, Math.PI * 2);
    return path;
  }
  if (shape.kind === "polygon")
    return rounded(
      Array.from({ length: shape.sides }, (_, i) => {
        const a = ((shape.rotation + (i * 360) / shape.sides) * Math.PI) / 180;
        return [cx + r * Math.cos(a), cy + r * Math.sin(a)];
      }),
      shape.round,
    );
  if (shape.kind === "rect") {
    const w = shape.width / 2,
      h = shape.height / 2,
      skew = (shape.skew ?? 0) / 2;
    return rounded(
      [
        [cx - w + skew, cy - h],
        [cx + w + skew, cy - h],
        [cx + w - skew, cy + h],
        [cx - w - skew, cy + h],
      ],
      shape.round,
    );
  }
  if (shape.kind === "bob") {
    const at = (u: number, v: number): Point => [cx + u * r, cy + v * r];
    path.moveTo(...at(-1.14, 0.98));
    path.quadraticCurveTo(...at(-0.92, 0.72), ...at(-0.98, 0.3));
    for (let step = 0; step <= 48; step++) {
      const a = Math.PI + (step / 48) * Math.PI;
      path.lineTo(...at(0.98 * Math.cos(a), -0.08 + 0.96 * Math.sin(a)));
    }
    path.lineTo(...at(0.98, 0.3));
    path.quadraticCurveTo(...at(0.92, 0.72), ...at(1.14, 0.98));
    path.lineTo(...at(0.62, 0.9));
    path.quadraticCurveTo(...at(0, 1.06), ...at(-0.62, 0.9));
    path.closePath();
    return path;
  }
  path.ellipse(cx, cy + 0.1 * r, 1.02 * r, 0.88 * r, 0, 0, Math.PI * 2);
  for (const side of [-1, 1]) {
    path.moveTo(cx + side * 0.94 * r, cy - 0.02 * r);
    path.lineTo(cx + side * 0.78 * r, cy - 1.04 * r);
    path.lineTo(cx + side * 0.16 * r, cy - 0.6 * r);
    path.closePath();
  }
  return path;
}
function reaches(pixels: Uint8ClampedArray, center: Point): number[] {
  return Array.from({ length: 360 }, (_, i) => {
    const a = (i * Math.PI) / 180,
      dx = Math.cos(a),
      dy = Math.sin(a);
    let last = 0;
    for (let d = 0; d < 384; d++) {
      const x = Math.round(center[0] * 256 + dx * d),
        y = Math.round(center[1] * 256 + dy * d);
      if (x < 0 || y < 0 || x >= 256 || y >= 256) break;
      if (pixels[(y * 256 + x) * 4 + 3] > 127) last = d;
    }
    return Math.max(0.02, last / 256);
  });
}
function bodyReaches(s: BotSpec, logo: number[]): number[] {
  if (s.shape.kind === "circle") return Array(360).fill(s.radius);
  if (s.shape.kind === "outline") {
    let current = logo;
    const width = s.shape.smooth;
    for (let pass = 0; pass < 2; pass++)
      current = current.map((_, i) => {
        let sum = 0;
        for (let off = -width; off <= width; off++)
          sum += current[(i + off + 360) % 360];
        return sum / (width * 2 + 1);
      });
    const mean = current.reduce((a, b) => a + b, 0) / 360,
      keep = s.shape.keep;
    return current.map((value) =>
      Math.max(s.radius * 0.55, s.radius * (1 + keep * (value / mean - 1))),
    );
  }
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = 256;
  const c = canvas.getContext("2d")!;
  c.scale(256, 256);
  c.fill(primitive(s));
  if (s.shape.kind === "cat") {
    c.lineJoin = "round";
    c.lineWidth = 0.16 * s.radius;
    const [cx, cy] = s.center;
    for (const side of [-1, 1]) {
      const p = new Path2D();
      p.moveTo(cx + side * 0.94 * s.radius, cy - 0.02 * s.radius);
      p.lineTo(cx + side * 0.78 * s.radius, cy - 1.04 * s.radius);
      p.lineTo(cx + side * 0.16 * s.radius, cy - 0.6 * s.radius);
      p.closePath();
      c.stroke(p);
    }
  }
  return reaches(c.getImageData(0, 0, 256, 256).data, s.center);
}

export interface BotForm {
  key: string;
  spec: BotSpec;
  artwork: HTMLCanvasElement;
  field: HTMLCanvasElement | null;
  stripped: HTMLCanvasElement | null;
  top: number;
  logoTop: number;
  ink: string;
  logo: number[];
  body: number[];
  light: boolean;
}
const cache = new WeakMap<HTMLCanvasElement, Map<string, BotForm | null>>();
export function makeForm(
  key: string,
  artwork: HTMLCanvasElement,
  light: boolean,
): BotForm | null {
  let forms = cache.get(artwork);
  if (!forms) {
    forms = new Map();
    cache.set(artwork, forms);
  }
  const cacheKey = key + String(light);
  if (forms.has(cacheKey)) return forms.get(cacheKey)!;
  const s = botSpecs(light)[key];
  if (!s) return null;
  const logo = reaches(
    artwork.getContext("2d")!.getImageData(0, 0, 256, 256).data,
    s.center,
  );
  const body = bodyReaches(s, logo);
  const highest = (ring: number[]) =>
    Math.min(
      ...ring.map(
        (value, i) => s.center[1] + Math.sin((i * Math.PI) / 180) * value,
      ),
    );
  let stripped: HTMLCanvasElement | null = null;
  if (s.ownEyes) {
    stripped = document.createElement("canvas");
    stripped.width = stripped.height = 256;
    const c = stripped.getContext("2d")!;
    c.drawImage(artwork, 0, 0);
    for (const [x0, y0, x1, y1] of s.ownEyes)
      c.clearRect(
        x0 * 256 - 3,
        y0 * 256 - 3,
        (x1 - x0) * 256 + 6,
        (y1 - y0) * 256 + 6,
      );
  }
  const form: BotForm = {
    key,
    spec: s,
    artwork,
    field: colorField(artwork, s.unpainted ?? []),
    stripped,
    top: s.ownEyes ? highest(logo) : highest(body) + (s.settle ?? 0),
    logoTop: highest(logo),
    ink: light ? "#1d1d1f" : "#ffffff",
    logo,
    body,
    light,
  };
  forms.set(cacheKey, form);
  return form;
}
const outlines = new WeakMap<
  BotForm,
  { eased: number; turn: number; path: Path2D }
>();
export function formOutline(form: BotForm, eased: number, turn: number) {
  const cached = outlines.get(form);
  if (cached?.eased === eased && cached.turn === turn) return cached.path;
  const path = new Path2D(),
    blend = eased,
    [cx, cy] = form.spec.center;
  for (let i = 0; i < 360; i++) {
    const sample = (((i - turn) % 360) + 360) % 360,
      low = Math.floor(sample),
      part = sample - low;
    const body =
        form.body[low] + (form.body[(low + 1) % 360] - form.body[low]) * part,
      r = form.logo[i] + (body - form.logo[i]) * blend,
      a = (i * Math.PI) / 180;
    if (!i) path.moveTo(cx + Math.cos(a) * r, cy + Math.sin(a) * r);
    else path.lineTo(cx + Math.cos(a) * r, cy + Math.sin(a) * r);
  }
  path.closePath();
  outlines.set(form, { eased, turn, path });
  return path;
}
