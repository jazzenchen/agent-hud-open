/**
 * One WGSL paint function drawn over a transparent canvas with WebGPU, only
 * when something changed. The paint body sees each number and colour by name
 * and `uv` (0..1, top-left origin), and returns straight (not premultiplied)
 * colour in linear sRGB. Colours arrive as CSS strings and are decoded to
 * linear sRGB; the output is encoded back to sRGB and premultiplied, as the
 * canvas expects.
 */
export type PaintValues = Record<string, number | string>;

export interface GpuPaint {
  /** Sends the values that differ and draws one frame if any did. */
  update(values: PaintValues): void;
  dispose(): void;
}

export interface GpuPaintOptions {
  canvas: HTMLCanvasElement;
  numbers: readonly string[];
  colors: readonly string[];
  paint: string;
  values: PaintValues;
  /** After the first frame reaches the GPU. */
  onReady(): void;
  /** No WebGPU, a failed shader, or a lost device. */
  onUnavailable(): void;
}

// GPUBufferUsage flags; the DOM lib in use does not declare the namespace.
const UNIFORM = 0x40;
const COPY_DST = 0x08;

let sharedDevice: Promise<GPUDevice | null> | undefined;

/** Every glow shares one device; a lost one is replaced on the next request. */
function device(): Promise<GPUDevice | null> {
  sharedDevice ??= (async () => {
    const adapter = await navigator.gpu?.requestAdapter();
    const created = await adapter?.requestDevice();
    if (!created) return null;
    void created.lost.then(() => {
      sharedDevice = undefined;
    });
    return created;
  })().catch(() => null);
  return sharedDevice;
}

function layout(numbers: readonly string[], colors: readonly string[]) {
  // Colours first (16-byte aligned), then the canvas size, then the numbers.
  const resolution = colors.length * 4;
  const offsets = new Map<string, number>();
  colors.forEach((name, index) => offsets.set(name, index * 4));
  numbers.forEach((name, index) => offsets.set(name, resolution + 2 + index));
  const floats = Math.ceil((resolution + 2 + numbers.length) / 4) * 4;
  const fields = [
    ...colors.map((name) => `${name}: vec4f,`),
    "resolution: vec2f,",
    ...numbers.map((name) => `${name}: f32,`),
  ].join("\n  ");
  const names = [...colors, ...numbers]
    .map((name) => `let ${name} = paintUniforms.${name};`)
    .join("\n  ");
  return { offsets, resolution, floats, fields, names };
}

function shader(fields: string, names: string, paint: string) {
  return /* wgsl */ `
struct PaintUniforms {
  ${fields}
}
@group(0) @binding(0) var<uniform> paintUniforms: PaintUniforms;

@vertex fn vertexMain(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
  var corners = array<vec2f, 3>(vec2f(-1.0, -3.0), vec2f(-1.0, 1.0), vec2f(3.0, 1.0));
  return vec4f(corners[index], 0.0, 1.0);
}

fn paintAt(uv: vec2f) -> vec4f {
  ${names}
  ${paint}
}

fn encodeSrgb(color: vec3f) -> vec3f {
  return select(pow(color, vec3f(0.41666)) * 1.055 - 0.055, color * 12.92, color <= vec3f(0.0031308));
}

@fragment fn fragmentMain(@builtin(position) at: vec4f) -> @location(0) vec4f {
  let color = paintAt(at.xy / paintUniforms.resolution);
  return vec4f(encodeSrgb(color.rgb) * color.a, color.a);
}
`;
}

/** CSS colour → linear sRGB, through a one-pixel 2D canvas. */
function linearColor(css: string): [number, number, number, number] {
  const probe = document.createElement("canvas");
  probe.width = probe.height = 1;
  const context = probe.getContext("2d", { willReadFrequently: true });
  if (!context) return [0, 0, 0, 0];
  context.fillStyle = css;
  context.fillRect(0, 0, 1, 1);
  const [r, g, b, a] = context.getImageData(0, 0, 1, 1).data;
  const decode = (channel: number) => {
    const c = channel / 255;
    return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  return [decode(r), decode(g), decode(b), a / 255];
}

export function startGpuPaint(options: GpuPaintOptions): GpuPaint {
  const { canvas, numbers, colors, paint } = options;
  const plan = layout(numbers, colors);
  const data = new Float32Array(plan.floats);
  const sent: PaintValues = {};
  let disposed = false;
  let frame = 0;
  let drawn = false;
  let draw: (() => void) | undefined;
  let observer: ResizeObserver | undefined;
  let buffer: GPUBuffer | undefined;

  const write = (values: PaintValues) => {
    let changed = false;
    for (const key in values) {
      const offset = plan.offsets.get(key);
      const value = values[key];
      if (offset === undefined || sent[key] === value) continue;
      sent[key] = value;
      if (typeof value === "string") data.set(linearColor(value), offset);
      else data[offset] = value;
      changed = true;
    }
    return changed;
  };
  const request = () => {
    if (disposed || !draw || frame) return;
    frame = requestAnimationFrame(() => {
      frame = 0;
      draw?.();
    });
  };
  const unavailable = () => {
    if (!disposed) options.onUnavailable();
  };

  write(options.values);
  void (async () => {
    const gpu = await device();
    if (disposed) return;
    const context = canvas.getContext("webgpu") as GPUCanvasContext | null;
    if (!gpu || !context) return unavailable();
    const format = navigator.gpu.getPreferredCanvasFormat();
    context.configure({ device: gpu, format, alphaMode: "premultiplied" });
    let pipeline: GPURenderPipeline;
    try {
      const module = gpu.createShaderModule({
        code: shader(plan.fields, plan.names, paint),
      });
      pipeline = await gpu.createRenderPipelineAsync({
        layout: "auto",
        vertex: { module, entryPoint: "vertexMain" },
        fragment: { module, entryPoint: "fragmentMain", targets: [{ format }] },
      });
    } catch {
      return unavailable();
    }
    if (disposed) return context.unconfigure();
    buffer = gpu.createBuffer({
      size: data.byteLength,
      usage: UNIFORM | COPY_DST,
    });
    const group = gpu.createBindGroup({
      layout: pipeline.getBindGroupLayout(0),
      entries: [{ binding: 0, resource: { buffer } }],
    });
    void gpu.lost.then(() => {
      draw = undefined;
      unavailable();
    });
    draw = () => {
      if (!buffer || !canvas.width || !canvas.height) return;
      data[plan.resolution] = canvas.width;
      data[plan.resolution + 1] = canvas.height;
      gpu.queue.writeBuffer(buffer, 0, data);
      const encoder = gpu.createCommandEncoder();
      const pass = encoder.beginRenderPass({
        colorAttachments: [
          {
            view: context.getCurrentTexture().createView(),
            clearValue: { r: 0, g: 0, b: 0, a: 0 },
            loadOp: "clear",
            storeOp: "store",
          },
        ],
      });
      pass.setPipeline(pipeline);
      pass.setBindGroup(0, group);
      pass.draw(3);
      pass.end();
      gpu.queue.submit([encoder.finish()]);
      if (!drawn) {
        drawn = true;
        void gpu.queue.onSubmittedWorkDone().then(() => {
          if (!disposed) options.onReady();
        });
      }
    };
    // The backing store follows the element's size at the display's scale.
    observer = new ResizeObserver(([entry]) => {
      const scale = window.devicePixelRatio || 1;
      const box = entry.devicePixelContentBoxSize?.[0];
      const width = box
        ? box.inlineSize
        : Math.round(entry.contentRect.width * scale);
      const height = box
        ? box.blockSize
        : Math.round(entry.contentRect.height * scale);
      if (width === canvas.width && height === canvas.height && drawn) return;
      canvas.width = Math.max(1, width);
      canvas.height = Math.max(1, height);
      request();
    });
    try {
      observer.observe(canvas, { box: "device-pixel-content-box" });
    } catch {
      observer.observe(canvas);
    }
  })();

  return {
    update(values) {
      if (write(values)) request();
    },
    dispose() {
      disposed = true;
      draw = undefined;
      cancelAnimationFrame(frame);
      observer?.disconnect();
      buffer?.destroy();
    },
  };
}
