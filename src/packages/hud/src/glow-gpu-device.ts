/**
 * What every WebGPU paint on a page shares: one device, the pipelines
 * compiled on it, and the canvases paints gave back for the next one.
 */
/** The device every glow shares, with the paints to tell if it is lost and
 * the pipelines compiled on it, one per shader. */
export interface SharedDevice {
  gpu: GPUDevice;
  lost: Set<() => void>;
  pipelines: Map<string, Promise<GPURenderPipeline>>;
}

let sharedDevice: Promise<SharedDevice | null> | undefined;

/** Every glow shares one device; a lost one is replaced on the next request.
 * Paints hear of its loss through `lost`, which they leave when they go:
 * `gpu.lost` settles only if the device is lost, so a reaction of each paint's
 * own on it kept every paint that ever drew, with its canvas, for good. */
export function device(): Promise<SharedDevice | null> {
  sharedDevice ??= (async () => {
    const adapter = await navigator.gpu?.requestAdapter();
    const gpu = await adapter?.requestDevice();
    if (!gpu) return null;
    const shared: SharedDevice = { gpu, lost: new Set(), pipelines: new Map() };
    void gpu.lost.then(() => {
      sharedDevice = undefined;
      for (const lost of [...shared.lost]) lost();
      shared.lost.clear();
    });
    return shared;
  })().catch(() => null);
  return sharedDevice;
}

/** The pipeline that draws `code`, compiled once per device and format. */
export function pipeline(
  shared: SharedDevice,
  code: string,
  format: GPUTextureFormat,
): Promise<GPURenderPipeline> {
  const key = `${format}\n${code}`;
  let compiled = shared.pipelines.get(key);
  if (!compiled) {
    const module = shared.gpu.createShaderModule({ code });
    compiled = shared.gpu.createRenderPipelineAsync({
      layout: "auto",
      vertex: { module, entryPoint: "vertexMain" },
      fragment: { module, entryPoint: "fragmentMain", targets: [{ format }] },
    });
    shared.pipelines.set(key, compiled);
    compiled.catch(() => shared.pipelines.delete(key));
  }
  return compiled;
}

/** Canvases a paint drew on and gave back, for the next paint. WebKit keeps a
 * canvas it has configured for WebGPU, with its drawing surfaces, for as long
 * as the page lives: unconfiguring it, destroying its textures or even its
 * device frees none of it. The Dynamic dock's rim goes away under every card
 * and comes back after, so a new canvas each time grew the HUD's web process
 * by 2.6 MB a card for as long as the app ran (issue 47). Reused, a page keeps
 * no more canvases than the rims it shows at once. */
const spareCanvases: HTMLCanvasElement[] = [];

/** A canvas for a new paint: one a paint gave back, or a new one. */
export function takeCanvas(): HTMLCanvasElement {
  return spareCanvases.pop() ?? document.createElement("canvas");
}

/** Keeps a paint's canvas, out of the page, for the next paint. */
export function giveBack(canvas: HTMLCanvasElement) {
  canvas.remove();
  spareCanvases.push(canvas);
}
