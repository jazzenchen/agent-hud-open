import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import type { UserConfig } from "vite";
import { devBridge } from "./dev-bridge.ts";

export { devBridge };

/** The Vite configuration every Agent HUD desktop front end shares: React,
 * Tailwind and the browser dev bridge to the
 * engine. One React serves the app and every package it imports; a product
 * that links these packages from another checkout names its other shared
 * dependencies in `dedupe`, so they too resolve from its own root. */
export function agentHudVite(options: { dedupe?: string[] } = {}): UserConfig {
  return {
    plugins: [react(), tailwindcss(), devBridge()],
    resolve: { dedupe: ["react", "react-dom", ...(options.dedupe ?? [])] },
    server: { port: 1420, strictPort: true },
    clearScreen: false,
  };
}
