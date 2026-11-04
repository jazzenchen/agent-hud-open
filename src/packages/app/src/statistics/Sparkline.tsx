import { memo, useCallback, useEffect, useMemo, useRef } from "react";
import { useCanvas } from "./use-canvas";

export const Sparkline = memo(function Sparkline({
  values,
  color,
}: {
  values: number[];
  color: string;
}) {
  const height = 28;
  const { ref, width, context } = useCanvas(height);
  const targetRatios = useMemo(() => {
    const peak = Math.max(0, ...values);
    return peak > 0
      ? values.map((v) => Math.max(0, v / peak))
      : values.map(() => 0);
  }, [values]);
  const currentRatiosRef = useRef<number[] | null>(null);

  const paint = useCallback(
    (ratios: number[]) => {
      const ctx = context();
      if (!ctx || !ratios.length) return;
      const slot = width / ratios.length;
      ctx.fillStyle = color;
      ctx.globalAlpha = 0.85;
      for (const [index, ratio] of ratios.entries()) {
        if (ratio > 0.001) {
          const h = Math.max(1.5, height * ratio);
          ctx.fillRect(
            index * slot + slot * 0.2,
            height - h,
            Math.max(1, slot * 0.6),
            h,
          );
        }
      }
    },
    [color, context, width],
  );

  useEffect(() => {
    if (!width) return;
    const prev = currentRatiosRef.current;
    if (
      prev === null ||
      matchMedia("(prefers-reduced-motion: reduce)").matches
    ) {
      currentRatiosRef.current = targetRatios;
      paint(targetRatios);
      return;
    }
    const from = prev;
    const duration = 240;
    const startTime = performance.now();
    let frame = 0;
    const animate = (now: number) => {
      const t = Math.min(1, Math.max(0, (now - startTime) / duration));
      const eased = 1 - Math.pow(1 - t, 3);
      const next = targetRatios.map((target, idx) => {
        const src =
          from.length === targetRatios.length
            ? (from[idx] ?? 0)
            : (from[
                Math.min(
                  from.length - 1,
                  Math.floor(
                    (idx / Math.max(1, targetRatios.length)) * from.length,
                  ),
                )
              ] ?? 0);
        return src + (target - src) * eased;
      });
      currentRatiosRef.current = next;
      paint(next);
      if (t < 1) {
        frame = requestAnimationFrame(animate);
      }
    };
    frame = requestAnimationFrame(animate);
    return () => cancelAnimationFrame(frame);
  }, [targetRatios, paint, width]);

  return (
    <canvas
      ref={ref}
      aria-hidden
      style={{ width: "100%", height, display: "block" }}
    />
  );
});
