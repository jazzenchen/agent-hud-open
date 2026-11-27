import { agentArtwork, agentArtworkKey } from "../agent-artwork";
export const botKey = agentArtworkKey;
const images = new Map<string, Promise<HTMLImageElement>>();
export function artworkURL(key: string, light: boolean) {
  return agentArtwork(key, light)?.url ?? null;
}
export function loadArtwork(
  key: string,
  light: boolean,
): Promise<HTMLCanvasElement | null> {
  const artwork = agentArtwork(key, light);
  if (!artwork) return Promise.resolve(null);
  const url = artwork.url;
  let loading = images.get(url);
  if (!loading) {
    loading = new Promise((resolve, reject) => {
      const image = new Image();
      image.onload = () => resolve(image);
      image.onerror = () =>
        reject(new Error(`Cannot load bot artwork: ${key}`));
      image.src = url;
    });
    images.set(url, loading);
  }
  return loading.then((image) => {
    const canvas = document.createElement("canvas");
    canvas.width = canvas.height = 256;
    const context = canvas.getContext("2d")!;
    // The default smoothing steps on edges when shrinking this much.
    context.imageSmoothingQuality = "high";
    context.drawImage(image, 0, 0, 256, 256);
    if (artwork.tint) {
      context.globalCompositeOperation = "source-in";
      context.fillStyle = artwork.tint;
      context.fillRect(0, 0, 256, 256);
      context.globalCompositeOperation = "source-over";
    }
    return canvas;
  });
}
