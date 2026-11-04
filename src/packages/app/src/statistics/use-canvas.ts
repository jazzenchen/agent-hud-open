import { useCallback, useEffect, useRef, useState } from "react";

/** A canvas that follows its laid-out width, and `context()`: its 2D context
 * sized for `height` on this screen's pixel ratio and cleared, or null before
 * the canvas is laid out. */
export function useCanvas(height: number) {
  const ref = useRef<HTMLCanvasElement>(null);
  const [width, setWidth] = useState(0);
  useEffect(() => {
    const node = ref.current;
    if (!node) return;
    const observer = new ResizeObserver(([entry]) =>
      setWidth((old) => {
        const next = Math.round(entry.contentRect.width);
        if (next <= 0 && old > 0) return old;
        return old === next ? old : next;
      }),
    );
    observer.observe(node);
    return () => observer.disconnect();
  }, []);
  const context = useCallback(() => {
    const canvas = ref.current,
      ctx = canvas?.getContext("2d");
    if (!canvas || !ctx || width <= 0) return null;
    const scale = window.devicePixelRatio || 1;
    const targetW = Math.round(width * scale),
      targetH = Math.round(height * scale);
    if (canvas.width !== targetW || canvas.height !== targetH) {
      canvas.width = targetW;
      canvas.height = targetH;
    }
    ctx.setTransform(scale, 0, 0, scale, 0, 0);
    ctx.clearRect(0, 0, width, height);
    return ctx;
  }, [height, width]);
  return { ref, width, context };
}
