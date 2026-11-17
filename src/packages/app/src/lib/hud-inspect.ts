import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { InspectView } from "@agent-hud/protocol";
import type { InspectServices } from "@agent-hud/hud/PanelContext";

/** One request to the host's clipboard window (`inspect_request`). */
const ask = <T>(method: string, params: Record<string, unknown> = {}) =>
  invoke<T>("inspect_request", { method, params });

/** The clipboard window's host requests, and its state as every HUD window
 * hears it (`hud:inspect`). */
export const hudInspect: InspectServices | undefined = isTauri()
  ? {
      state: () => ask("state"),
      subscribe: (listener) => {
        const subscription = listen<InspectView>("hud:inspect", ({ payload }) =>
          listener(payload),
        );
        return () => void subscription.then((unlisten) => unlisten());
      },
      look: () => ask("look"),
      add: () => ask("add"),
      edit: (id, text) => ask("edit", { id, text }),
      remove: (id) => ask("remove", { id }),
      clear: () => ask("clear"),
      targets: () => ask("targets"),
      markPicture: (id) => ask("mark_window", { id }),
      send: (target) => ask("send", { target }),
    }
  : undefined;
