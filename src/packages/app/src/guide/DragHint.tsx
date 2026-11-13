// A small screen in the guide's mode step: a hand takes the HUD from the
// top of the screen to its right edge, where it turns upright, and lets go.
// Still when animations are off.
import { Grab } from "lucide-react";

const keyframes = `
@keyframes hud-guide-drag {
  0%, 18% { left: 50%; top: 8px; transform: translate(-50%, -50%) rotate(0deg); }
  58%, 84% { left: calc(100% - 8px); top: 50%; transform: translate(-50%, -50%) rotate(90deg); }
  100% { left: 50%; top: 8px; transform: translate(-50%, -50%) rotate(0deg); }
}
@keyframes hud-guide-hand {
  0% { left: 50%; top: 8px; opacity: 0; }
  12%, 18% { left: 50%; top: 8px; opacity: 1; }
  58%, 70% { left: calc(100% - 8px); top: 50%; opacity: 1; }
  80%, 100% { left: calc(100% - 8px); top: 50%; opacity: 0; }
}`;

export function DragHint({ moving }: { moving: boolean }) {
  const loop = "3.6s cubic-bezier(0.45, 0, 0.25, 1) infinite";
  return (
    <div
      aria-hidden
      style={{
        position: "relative",
        height: 74,
        margin: "10px 0",
        borderRadius: 10,
        border: "1px solid rgba(128,128,128,0.3)",
        background: "rgba(128,128,128,0.08)",
        overflow: "hidden",
      }}
    >
      <style>{keyframes}</style>
      <div
        style={{
          position: "absolute",
          left: "50%",
          top: 8,
          width: 42,
          height: 11,
          borderRadius: 6,
          background: "rgba(128,128,128,0.55)",
          transform: "translate(-50%, -50%)",
          animation: moving ? `hud-guide-drag ${loop}` : undefined,
        }}
      />
      <Grab
        size={18}
        strokeWidth={2}
        style={{
          position: "absolute",
          left: "50%",
          top: 8,
          marginLeft: -2,
          marginTop: 2,
          animation: moving ? `hud-guide-hand ${loop}` : undefined,
        }}
      />
    </div>
  );
}
