import { test } from "node:test";
import assert from "node:assert/strict";
import { startGpuPaint } from "../src/glow-gpu.ts";

// Just enough of the DOM and WebGPU for startGpuPaint, counting what it makes.
const made = { gpuCanvases: new Set<unknown>(), devices: 0, pipelines: 0 };
let loseDevice = () => {};

class FakeContext {
  configured = false;
  configure() {
    this.configured = true;
  }
  unconfigure() {
    this.configured = false;
  }
  getCurrentTexture() {
    return { createView: () => ({}) };
  }
}

class FakeCanvas {
  width = 300;
  height = 150;
  style = { cssText: "" };
  dataset: Record<string, string> = {};
  parent: FakeHost | null = null;
  webgpu = new FakeContext();
  getContext(kind: string) {
    if (kind === "webgpu") {
      made.gpuCanvases.add(this);
      return this.webgpu;
    }
    return {
      fillStyle: "",
      fillRect() {},
      getImageData: () => ({ data: [255, 255, 255, 255] }),
    };
  }
  remove() {
    if (this.parent)
      this.parent.children = this.parent.children.filter((c) => c !== this);
    this.parent = null;
  }
}

class FakeHost {
  children: FakeCanvas[] = [];
  append(canvas: FakeCanvas) {
    canvas.remove();
    canvas.parent = this;
    this.children.push(canvas);
  }
}

function fakeDevice() {
  made.devices++;
  let lose = () => {};
  const lost = new Promise<void>((resolve) => (lose = resolve));
  loseDevice = lose;
  return {
    lost,
    createShaderModule: () => ({}),
    createRenderPipelineAsync: async () => {
      made.pipelines++;
      return { getBindGroupLayout: () => ({}) };
    },
    createBuffer: () => ({ destroy() {} }),
    createBindGroup: () => ({}),
    createCommandEncoder: () => ({
      beginRenderPass: () => ({
        setPipeline() {},
        setBindGroup() {},
        draw() {},
        end() {},
      }),
      finish: () => ({}),
    }),
    queue: {
      writeBuffer() {},
      submit() {},
      onSubmittedWorkDone: async () => {},
    },
  };
}

Object.assign(globalThis, {
  window: globalThis,
  devicePixelRatio: 2,
  document: {
    createElement: () => new FakeCanvas(),
  },
  requestAnimationFrame: (run: () => void) => setTimeout(run, 0),
  cancelAnimationFrame: (id: ReturnType<typeof setTimeout>) => clearTimeout(id),
  ResizeObserver: class {
    changed: (entries: { devicePixelContentBoxSize: unknown[] }[]) => void;
    constructor(
      changed: (entries: { devicePixelContentBoxSize: unknown[] }[]) => void,
    ) {
      this.changed = changed;
    }
    observe() {
      const box = { inlineSize: 1404, blockSize: 156 };
      queueMicrotask(() =>
        this.changed([{ devicePixelContentBoxSize: [box] }]),
      );
    }
    disconnect() {}
  },
});
Object.defineProperty(globalThis, "navigator", {
  configurable: true,
  value: {
    gpu: {
      requestAdapter: async () => ({ requestDevice: async () => fakeDevice() }),
      getPreferredCanvasFormat: () => "bgra8unorm",
    },
  },
});

function rim(host: FakeHost) {
  const told = { ready: 0, unavailable: 0 };
  let ready = () => {};
  const shown = new Promise<void>((resolve) => (ready = resolve));
  const paint = startGpuPaint({
    host: host as unknown as HTMLElement,
    numbers: ["strength"],
    colors: ["colorA"],
    paint: "return vec4f(colorA.rgb, strength);",
    values: { strength: 1, colorA: "#ffffff" },
    onReady: () => {
      told.ready++;
      ready();
    },
    onUnavailable: () => told.unavailable++,
  });
  return { paint, told, shown };
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 5));

test("a rim that comes back draws on the canvas the last one gave back", async () => {
  // WebKit never frees a canvas once configured for WebGPU (issue 47): the
  // Dynamic dock's rim, gone under every card, must not make a new one each time.
  const first = new FakeHost();
  const a = rim(first);
  await a.shown;
  const canvas = first.children[0];
  assert.equal(canvas.webgpu.configured, true);
  assert.equal(canvas.width, 1404);
  a.paint.dispose();
  assert.deepEqual(first.children, []);
  assert.equal(canvas.webgpu.configured, false);

  for (let card = 0; card < 20; card++) {
    const host = new FakeHost();
    const b = rim(host);
    await b.shown;
    assert.equal(host.children[0], canvas);
    assert.equal(canvas.webgpu.configured, true);
    b.paint.dispose();
  }
  assert.equal(made.gpuCanvases.size, 1, "one WebGPU canvas for 21 rims");
  assert.equal(made.pipelines, 1, "one pipeline, compiled once");

  // Two rims at once take two canvases; both come back.
  const one = rim(new FakeHost());
  const two = rim(new FakeHost());
  await Promise.all([one.shown, two.shown]);
  one.paint.dispose();
  two.paint.dispose();
  one.paint.dispose();
  const again = [new FakeHost(), new FakeHost(), new FakeHost()];
  const three = again.map(rim);
  await Promise.all(three.map((r) => r.shown));
  const used = new Set(again.map((h) => h.children[0]));
  assert.equal(used.size, 3);
  assert.ok(used.has(canvas));
  for (const r of three) r.paint.dispose();
});

test("only the rims still drawing hear that the device was lost", async () => {
  const devicesBefore = made.devices;
  const gone = rim(new FakeHost());
  const kept = rim(new FakeHost());
  await Promise.all([gone.shown, kept.shown]);
  gone.paint.dispose();
  loseDevice();
  await settle();
  assert.equal(gone.told.unavailable, 0);
  assert.equal(kept.told.unavailable, 1);
  kept.paint.dispose();

  // The next rim asks for a new device and compiles on it.
  const pipelinesBefore = made.pipelines;
  const next = rim(new FakeHost());
  await next.shown;
  assert.equal(made.devices, devicesBefore + 1);
  assert.equal(made.pipelines, pipelinesBefore + 1);
  next.paint.dispose();
});
