import { isTauri } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import type { EndsLink, EndsOp } from "@agent-hud/hud/PanelContext";

/** One window's own mark on what it sends, so it does not hear itself. */
const self =
  globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random()}`;

/** The dock ends' link between the HUD windows of every screen, over app
 * events: each window sends what happened at its ends and hears the rest. */
export const hudEndsLink: EndsLink | undefined = isTauri()
  ? {
      send: (op) =>
        void emit("hud:ends", { from: self, op }).catch((error) =>
          console.error("HUD ends could not be shared", error),
        ),
      listen: (handle) => {
        const subscription = listen<{ from: string; op: EndsOp }>(
          "hud:ends",
          ({ payload }) => {
            if (payload.from !== self) handle(payload.op, payload.from);
          },
        );
        return () => void subscription.then((unlisten) => unlisten());
      },
    }
  : undefined;
