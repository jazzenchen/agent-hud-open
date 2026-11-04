import type { Box } from "./bot-spec";
/** Original 60x60 colour-field spread and three box-blur passes. */
export function colorField(
  artwork: HTMLCanvasElement,
  holes: Box[],
): HTMLCanvasElement | null {
  const input = artwork.getContext("2d")!.getImageData(0, 0, 256, 256).data;
  const values = new Float32Array(60 * 60 * 3),
    known = new Uint8Array(60 * 60);
  let seen = 0;
  const low = [255, 255, 255],
    high = [0, 0, 0],
    sum = [0, 0, 0];
  for (let y = 0; y < 40; y++)
    for (let x = 0; x < 40; x++) {
      const source =
        (Math.floor(((y + 0.5) * 256) / 40) * 256 +
          Math.floor(((x + 0.5) * 256) / 40)) *
        4;
      if (
        input[source + 3] <= 200 ||
        holes.some(
          ([x0, y0, x1, y1]) =>
            (x + 0.5) / 40 >= x0 &&
            (x + 0.5) / 40 < x1 &&
            (y + 0.5) / 40 >= y0 &&
            (y + 0.5) / 40 < y1,
        )
      )
        continue;
      const target = (y + 10) * 60 + x + 10;
      known[target] = 1;
      for (let channel = 0; channel < 3; channel++) {
        const value = input[source + channel];
        values[target * 3 + channel] = value;
        low[channel] = Math.min(low[channel], value);
        high[channel] = Math.max(high[channel], value);
        sum[channel] += value;
      }
      seen++;
    }
  if (!seen) return null;
  if (high.every((value, i) => value - low[i] < 14)) {
    const swatch = document.createElement("canvas");
    swatch.width = swatch.height = 1;
    const c = swatch.getContext("2d")!;
    c.fillStyle = `rgb(${sum.map((value) => value / seen).join(",")})`;
    c.fillRect(0, 0, 1, 1);
    return swatch;
  }
  for (let vertical = 0; vertical < 2; vertical++) {
    const filled = known.slice(),
      at = (line: number, offset: number) =>
        vertical ? offset * 60 + line : line * 60 + offset;
    for (let row = 0; row < 60; row++) {
      let before = -1,
        position = 0;
      while (position < 60) {
        if (known[at(row, position)]) {
          before = position++;
          continue;
        }
        let after = position;
        while (after < 60 && !known[at(row, after)]) after++;
        if (before >= 0 || after < 60)
          for (let x = position; x < after; x++) {
            const from = before < 0 ? at(row, after) : at(row, before),
              to = after < 60 ? at(row, after) : from,
              share =
                before < 0
                  ? 1
                  : after >= 60
                    ? 0
                    : (x - before) / (after - before),
              index = at(row, x);
            for (let channel = 0; channel < 3; channel++)
              values[index * 3 + channel] =
                values[from * 3 + channel] * (1 - share) +
                values[to * 3 + channel] * share;
            filled[index] = 1;
          }
        position = after;
      }
    }
    known.set(filled);
  }
  const other = values.slice();
  for (let pass = 0; pass < 3; pass++) {
    for (let y = 0; y < 60; y++)
      for (let x = 0; x < 60; x++)
        for (let ch = 0; ch < 3; ch++)
          other[(y * 60 + x) * 3 + ch] =
            (values[(y * 60 + Math.max(0, x - 1)) * 3 + ch] +
              values[(y * 60 + x) * 3 + ch] +
              values[(y * 60 + Math.min(59, x + 1)) * 3 + ch]) /
            3;
    for (let y = 0; y < 60; y++)
      for (let x = 0; x < 60; x++)
        for (let ch = 0; ch < 3; ch++)
          values[(y * 60 + x) * 3 + ch] =
            (other[(Math.max(0, y - 1) * 60 + x) * 3 + ch] +
              other[(y * 60 + x) * 3 + ch] +
              other[(Math.min(59, y + 1) * 60 + x) * 3 + ch]) /
            3;
  }
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = 60;
  const c = canvas.getContext("2d")!,
    out = c.createImageData(60, 60);
  for (let i = 0; i < 3600; i++) {
    for (let ch = 0; ch < 3; ch++) out.data[i * 4 + ch] = values[i * 3 + ch];
    out.data[i * 4 + 3] = 255;
  }
  c.putImageData(out, 0, 0);
  return canvas;
}
