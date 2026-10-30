// The state behind Agent plugins: each plugin's and each client's status, the
// commands that change them, and the app-wide HUD tools switch.
import { useEffect, useRef, useState } from "react";
import type { NativeMethod, Snapshot } from "@agent-hud/protocol";
import { failure, toolsSwitch } from "../../lib/agent-plugins";
import type { Text } from "../SettingsPrimitives";
import {
  cache,
  type ClaudePluginService,
  type ClaudePluginStatus,
  type MCPClientsService,
  type MCPClientsStatus,
} from "./services";

type Action = (
  method: NativeMethod,
  params?: Record<string, unknown>,
) => Promise<unknown>;

/** A flag that falls back to null a moment after it is set ("Copied"). */
export function useBriefly<T>(ms: number) {
  const [value, setValue] = useState<T | null>(null);
  useEffect(() => {
    if (value === null) return;
    const timer = setTimeout(() => setValue(null), ms);
    return () => clearTimeout(timer);
  }, [value, ms]);
  return [value, setValue] as const;
}

/** Whether the window is in its light theme, following changes. */
export function useLightTheme() {
  const read = () =>
    typeof document !== "undefined" &&
    document.documentElement.dataset.theme === "light";
  const [light, setLight] = useState(read);
  useEffect(() => {
    const observer = new MutationObserver(() => setLight(read()));
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["data-theme"],
    });
    return () => observer.disconnect();
  }, []);
  return light;
}

/** A client plugin's status, read again every 10 seconds while Settings is on
 * screen (the loaded count follows sessions checking in) and when the app
 * comes to the front, and its install and remove. */
export function usePlugin(
  client: string,
  service: ClaudePluginService,
  active: boolean,
  t: Text,
) {
  const [status, setStatus] = useState<ClaudePluginStatus | null>(
    cache.plugins[client] ?? null,
  );
  const [running, setRunning] = useState<"install" | "remove" | null>(null);
  const [justInstalled, setJustInstalled] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const read = useRef(service);
  read.current = service;
  const apply = (next: ClaudePluginStatus) => {
    cache.plugins[client] = next;
    setStatus(next);
  };
  useEffect(() => {
    if (!active) return;
    let dead = false;
    const again = () =>
      void read
        .current("status")
        .then((next) => {
          if (!dead) apply(next);
        })
        .catch((failed) => {
          if (!dead) setError(String(failed));
        });
    again();
    const timer = setInterval(again, 10_000);
    window.addEventListener("focus", again);
    return () => {
      dead = true;
      clearInterval(timer);
      window.removeEventListener("focus", again);
    };
  }, [active]);
  async function change(kind: "install" | "remove") {
    setRunning(kind);
    setError(null);
    try {
      apply(await read.current(kind));
      setJustInstalled(kind === "install");
    } catch (failed) {
      setError(t(...failure(kind, String(failed))));
    } finally {
      setRunning(null);
    }
  }
  return { status, running, justInstalled, error, change };
}
export type PluginStatus = ReturnType<typeof usePlugin>;

/** Which clients have HUD tools, read when the page opens and when the app
 * comes to the front (so a command run in Terminal shows up), with adding,
 * removing and copying a client's command. */
export function useMcpClients(
  clients: MCPClientsService,
  active: boolean,
  t: Text,
) {
  const [status, setStatus] = useState<MCPClientsStatus | null>(cache.clients);
  const [running, setRunning] = useState<Record<string, "add" | "remove">>({});
  const [justAdded, setJustAdded] = useState<string[]>([]);
  const [copied, setCopied] = useBriefly<string>(4000);
  const [errors, setErrors] = useState<Record<string, string>>({});
  const service = useRef(clients);
  service.current = clients;
  const apply = (next: MCPClientsStatus) => {
    cache.clients = next;
    setStatus(next);
  };
  useEffect(() => {
    if (!active) return;
    let dead = false;
    const read = () =>
      void service
        .current("status")
        .then((next) => {
          if (!dead) apply(next);
        })
        .catch((error) => {
          if (!dead)
            apply({
              ready: false,
              notice: String(error),
              json: null,
              clients: [],
            });
        });
    read();
    window.addEventListener("focus", read);
    return () => {
      dead = true;
      window.removeEventListener("focus", read);
    };
  }, [active]);
  async function change(client: string, kind: "add" | "remove") {
    setRunning((previous) => ({ ...previous, [client]: kind }));
    setErrors(({ [client]: _, ...rest }) => rest);
    setCopied(null);
    try {
      apply(
        await service.current(kind === "add" ? "install" : "remove", client),
      );
      setJustAdded((previous) =>
        kind === "add"
          ? [...previous, client]
          : previous.filter((id) => id !== client),
      );
    } catch (error) {
      setErrors((previous) => ({
        ...previous,
        [client]: t(...failure(kind, String(error))),
      }));
    } finally {
      setRunning(({ [client]: _, ...rest }) => rest);
    }
  }
  function copyCommand(client: string, command: string) {
    void navigator.clipboard
      ?.writeText(command)
      .then(() => {
        // The copy is the next step now; an earlier failure would hide it.
        setErrors(({ [client]: _, ...rest }) => rest);
        setCopied(client);
      })
      .catch(() => {});
  }
  return { status, running, justAdded, copied, errors, change, copyCommand };
}
export type McpClients = ReturnType<typeof useMcpClients>;

/** The app-wide switch that lets agents with HUD tools arrange the HUD. */
export function useHudTools(snapshot: Snapshot, action: Action) {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const local = snapshot.connection.localTools;
  const on = Boolean(snapshot.settings.hudTools);
  const listening = Boolean(local?.listening);
  const state = toolsSwitch({
    supported: Boolean(local?.supported),
    enabled: on,
    listening,
    notice: local?.notice ?? null,
    demo: snapshot.mode === "demo",
  });
  async function set(value: boolean) {
    setPending(true);
    setError(null);
    try {
      await action("set_setting", { key: "hudTools", value });
    } catch (failed) {
      setError(String(failed));
    } finally {
      setPending(false);
    }
  }
  return { on, listening, state, pending, error, set };
}
export type HudTools = ReturnType<typeof useHudTools>;
