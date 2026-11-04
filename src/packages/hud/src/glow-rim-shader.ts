/**
 * The rim's WGSL paint body, drawn by glow-gpu.
 * Contour, Gaussian grid falloff and six clocks follow GlowMatrix / GlowMotion.
 * The output is transparent inside the HUD and outside the rim; it never paints a background.
 * `clock` is seconds since the rim appeared. GpuRim advances it only while a
 * pattern moves, so a resting or breathing rim is drawn once, not every frame.
 */
export const RIM_NUMBERS = [
  "stripWidth",
  "stripHeight",
  "padding",
  "corner",
  "shoulder",
  "attachedEdge",
  "pattern",
  "motionKind",
  "cycle",
  "depth",
  "strength",
  "reach",
  "outwardFalloff",
  "feather",
  "cellPitch",
  "coreRows",
  "fadeRows",
  "cellDensity",
  "clock",
  "stopCount",
  "boxWidth",
  "boxHeight",
  "offsetX",
  "offsetY",
] as const;
export const RIM_COLORS = ["colorA", "colorB", "colorC", "colorD"] as const;
export const RIM_PAINT = /* wgsl */ `
    // The canvas covers the box; the silhouette sits offsetX, offsetY into it.
    let box = vec2f(boxWidth, boxHeight) + vec2f(padding * 2.0);
    let pixel = uv * box - vec2f(offsetX, offsetY);
    let canvas = vec2f(stripWidth, stripHeight) + vec2f(padding * 2.0);
    let pixelPoint = pixel - vec2f(padding);
    let pixelRadius = min(corner, min(stripWidth, stripHeight) * 0.5);
    var pixelDistance: f32;
    if (attachedEdge > 0.5) {
      let centerX = clamp(pixelPoint.x, pixelRadius, stripWidth - pixelRadius);
      pixelDistance = length(vec2f(pixelPoint.x - centerX, max(pixelPoint.y - stripHeight + pixelRadius, 0.0))) - pixelRadius;
      if (pixelPoint.y < stripHeight - pixelRadius) { pixelDistance = max(-pixelPoint.x, pixelPoint.x - stripWidth); }
      let lip = min(shoulder, stripHeight * 0.5);
      if (lip > 0.0 && pixelPoint.y >= 0.0 && pixelPoint.y < lip) {
        let outside = lip * pow(1.0 - sqrt(pixelPoint.y / lip), 2.0);
        if (pixelPoint.x >= -outside && pixelPoint.x <= stripWidth + outside) { return vec4f(0.0); }
      }
    } else {
      let q = abs(pixelPoint - vec2f(stripWidth, stripHeight) * 0.5) - vec2f(stripWidth, stripHeight) * 0.5 + vec2f(pixelRadius);
      pixelDistance = length(max(q, vec2f(0.0))) + min(max(q.x, q.y), 0.0) - pixelRadius;
    }
    // Cell centres choose a mark; its actual pixels must remain outside the silhouette.
    if (pixelDistance < 0.0) { return vec4f(0.0); }
    let pitch = max(0.5, cellPitch);
    let rowPitch = select(pitch, pitch * 0.5, pattern > 3.5 && pattern < 4.5);
    let columnPitch = select(pitch, pitch * 0.5, pattern > 3.5 && pattern < 4.5);
    let cell = floor((pixel - vec2f(padding, padding + stripHeight)) / vec2f(columnPitch, rowPitch));
    var samplePoint = pixel;
    if (pattern > 0.5) {
      samplePoint = (cell + vec2f(0.5)) * vec2f(columnPitch, rowPitch) + vec2f(padding, padding + stripHeight);
    }
    let p = samplePoint - vec2f(padding);
    let r = min(corner, min(stripWidth, stripHeight) * 0.5);
    var distanceFromRim: f32;
    if (attachedEdge > 0.5) {
      let x = clamp(p.x, r, stripWidth - r);
      distanceFromRim = length(vec2f(p.x - x, max(p.y - stripHeight + r, 0.0))) - r;
      if (p.y < stripHeight - r) { distanceFromRim = max(-p.x, p.x - stripWidth); }
    } else {
      let q = abs(p - vec2f(stripWidth, stripHeight) * 0.5) - vec2f(stripWidth, stripHeight) * 0.5 + vec2f(r);
      distanceFromRim = length(max(q, vec2f(0.0))) + min(max(q.x, q.y), 0.0) - r;
    }
    if (distanceFromRim < 0.0) { return vec4f(0.0); }
    var value: f32;
    if (pattern < 0.5) {
      let tail = max(0.5, reach + feather * 3.0);
      let power = max(1.0, 8.0 * feather / max(0.5, reach));
      value = pow(max(0.0, 1.0 - distanceFromRim / tail), power);
      if (outwardFalloff < 0.5) {
        let featherWidth = min(reach, feather * 2.0);
        let t = clamp((distanceFromRim - reach + featherWidth) / max(0.5, featherWidth + feather * 3.0), 0.0, 1.0);
        value = (1.0 - t) * (1.0 - t);
      }
      if (feather <= 0.0) { value = select(0.0, 1.0, distanceFromRim <= reach); }
    } else {
      let fall = max(0.0, distanceFromRim / pitch - coreRows) / max(0.01, fadeRows);
      value = exp(-2.995732 * fall * fall);
      if (fall > 1.0) { return vec4f(0.0); }
    }
    let phase = clock / max(0.25, cycle);
    var gain = 1.0;
    var location = clamp(samplePoint.x / canvas.x, 0.0, 1.0);
    let noise = fract(sin(cell.x * 12.9898 + cell.y * 78.233 + floor(phase * 24.0) * 37.719) * 43758.5453);
    if (motionKind < 0.5) {
      gain = 1.0 - depth * (0.5 - 0.5 * cos(6.2831853 * phase));
    } else if (motionKind < 1.5) {
      location = 1.0 - abs(2.0 * fract(location * 0.5 + phase) - 1.0);
    } else if (motionKind < 2.5) {
      let center = fract(phase) * (canvas.x + 160.0 / 14.0 * pitch) - 80.0 / 14.0 * pitch;
      let offset = (samplePoint.x - center) / (40.0 / 14.0 * pitch);
      gain = 0.5 + 0.7 * exp(-offset * offset);
    } else if (motionKind < 3.5) {
      gain = 0.55 + 0.45 * (0.5 + 0.5 * sin(6.2831853 * (distanceFromRim / (3.0 * pitch) - phase)));
    } else if (motionKind < 4.5) {
      let flash = fract(sin(cell.x * 12.9898 + cell.y * 78.233 + (floor(phase * 24.0) + 53.0) * 37.719) * 43758.5453);
      gain = select(0.5 + 0.3 * noise, 1.2, flash < 0.12);
    } else {
      let boot = fract(phase) * 4.4;
      if (boot < 2.2) { gain = select(0.0, 1.0, distanceFromRim < boot / 2.2 * 8.0 * pitch); }
      else if (boot >= 3.6) { gain = 1.0 - (boot - 3.6) / 0.8; }
    }
    value = clamp(value * gain, 0.0, 1.0);
    if (value < 0.06) { return vec4f(0.0); }
    let gradientLocation = clamp(location * stopCount - 0.5, 0.0, stopCount - 1.0);
    var rgb = mix(colorA.rgb, colorB.rgb, clamp(gradientLocation, 0.0, 1.0));
    rgb = mix(rgb, colorC.rgb, clamp(gradientLocation - 1.0, 0.0, 1.0));
    rgb = mix(rgb, colorD.rgb, clamp(gradientLocation - 2.0, 0.0, 1.0));
    if (motionKind > 0.5 && motionKind < 1.5) {
      let offset = (samplePoint.x / canvas.x - (fract(phase) * 1.5 - 0.25)) / 0.25;
      let sheen = pow(cos(clamp(offset, -1.0, 1.0) * 1.5707963), 2.0);
      rgb = mix(rgb, vec3f(1.0), sheen * 0.3);
    }
    var alpha = value;
    if (pattern > 0.5) {
      let local = (pixel - samplePoint) / vec2f(columnPitch, rowPitch);
      let bayer = array<f32, 16>(0.0,8.0,2.0,10.0,12.0,4.0,14.0,6.0,3.0,11.0,1.0,9.0,15.0,7.0,13.0,5.0);
      let bx = u32(i32(cell.x) & 3); let by = u32(i32(cell.y) & 3);
      let threshold = (bayer[by * 4u + bx] + 0.5) / 16.0;
      var jitter = 0.0;
      let noiseStep = floor(phase * select(24.0, 35.2, motionKind > 4.5));
      if (motionKind > 3.5 && motionKind < 4.5) {
        let twinkle = fract(sin(cell.x * 12.9898 + cell.y * 78.233 + (noiseStep + 101.0) * 37.719) * 43758.5453);
        if (twinkle < 0.1) { jitter = 2.0; }
      } else if (motionKind > 4.5) {
        let boot = fract(phase) * 4.4;
        if (boot < 2.2 && abs(distanceFromRim - boot / 2.2 * 8.0 * pitch) < 0.8 * pitch) { jitter = 9.0; }
      }
      let shiftNoise = fract(sin(cell.x * 12.9898 + cell.y * 78.233 + (noiseStep + 7.0) * 37.719) * 43758.5453);
      let shift = min(jitter * 2.0, floor(shiftNoise * (jitter * 2.0 + 1.0))) - jitter;
      if (pattern < 1.5) {
        let dotRadius = cellDensity * (0.07 + 0.45 * sqrt(value));
        alpha = 1.0 - smoothstep(dotRadius - 0.045, dotRadius, length(local));
      } else if (pattern > 3.5 && pattern < 4.5) {
        alpha = select(0.0, 0.45 + 0.55 * sqrt(value), 0.1 + 0.9 * value > threshold);
        alpha *= 1.0 - smoothstep(cellDensity * 0.30, cellDensity * 0.34, length(local));
      } else if (pattern > 2.5 && pattern < 3.5) {
        let level = clamp(clamp(floor(value * 4.0 + threshold - 0.5), 0.0, 3.0) + shift, 0.0, 3.0);
        let micro = floor((local + vec2f(0.5)) * vec2f(4.0));
        let mark = fract((micro.x + micro.y * 3.0) / 4.0);
        alpha = select(0.0, 0.45 + 0.55 * sqrt(value), mark < (level + 1.0) * 0.25);
        if (max(abs(local.x), abs(local.y)) > cellDensity * 0.5) { alpha = 0.0; }
      } else {
        var letter = u32(clamp(clamp(floor(value * 9.0 + threshold - 0.5), 0.0, 8.0) + shift, 0.0, 8.0));
        if (pattern > 4.5) { letter = select(9u, 10u, 0.1 + 0.9 * value > threshold); }
        let glyphPoint = (local / max(0.1, cellDensity) + vec2f(0.5)) * vec2f(5.0, 7.0);
        if (any(glyphPoint < vec2f(0.0)) || any(glyphPoint >= vec2f(5.0,7.0))) { return vec4f(0.0); }
        let bit = u32(floor(glyphPoint.y)) * 5u + u32(floor(glyphPoint.x));
        let lo = array<u32, 11>(134217728u,138416256u,1015808u,32537600u,139432064u,720353952u,2515893578u,438440307u,2212165166u,2738546222u,2286031044u);
        let hi = array<u32, 11>(1u,0u,0u,0u,0u,0u,2u,3u,3u,3u,3u);
        let bits = select(lo[letter], hi[letter], bit >= 32u);
        alpha = f32((bits >> (bit % 32u)) & 1u) * (0.45 + 0.55 * sqrt(value));
        if (letter == 9u) { alpha *= 0.35; }
      }
    }
    return vec4f(rgb, alpha * strength);
`;
