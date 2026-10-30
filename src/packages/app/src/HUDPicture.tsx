import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Snapshot } from "@agent-hud/protocol";
import { FormatProvider } from "@agent-hud/ui/format-context";
import { snapshotLanguage } from "@agent-hud/ui/format";
import { PanelServicesProvider } from "@agent-hud/hud/PanelContext";
import { SourceHoverPanel } from "@agent-hud/hud/HoverPanel";
import { HUDScene, HUDLogo } from "@agent-hud/hud/HUDScene";
import { HUDGroundProvider } from "@agent-hud/hud/ground";
import { stripMetrics } from "@agent-hud/hud/geometry";
import {
  vendorKey,
  configuredVendors,
  quotaAccountKey,
} from "@agent-hud/hud/model";
import {
  getScreenHUDConfig,
  presentationStore,
  type PresentationState,
} from "./lib/use-hud-config";
interface Job {
  id: string;
  params: { screen?: string; show?: string; agent?: string };
  metadata: Record<string, unknown>;
  snapshot: Snapshot;
  presentation: PresentationState & {
    screens: { id: string; monitorIndex: number; hasNotch: boolean }[];
  };
}
/** Mask after filtering by the real account/project keys, before anything enters the DOM. */
export function pictureSnapshot(
  snapshot: Snapshot,
  config: ReturnType<typeof getScreenHUDConfig>,
): Snapshot {
  const blocks = config.panelLayout.blocks ?? {},
    projects = config.panelLayout.sessions?.projects;
  const mask = (text: string) => {
    const at = text.indexOf("@"),
      local = at < 0 ? text : text.slice(0, at);
    const chars = [...local];
    return (
      (chars.length > 2
        ? `${chars[0]}***${chars.at(-1)}`
        : chars.length
          ? "***"
          : "") + (at < 0 ? "" : text.slice(at))
    );
  };
  return {
    ...snapshot,
    approvals: [],
    requestDeliveries: [],
    quotas: snapshot.quotas
      .filter(
        (q) =>
          !(blocks[q.vendor]?.hiddenAccounts ?? []).includes(
            quotaAccountKey(q),
          ) &&
          (!blocks[q.vendor]?.accounts?.length ||
            blocks[q.vendor].accounts!.includes(quotaAccountKey(q))),
      )
      .map((q) => ({ ...q, account: q.account ? mask(q.account) : q.account })),
    accounts: snapshot.accounts?.map((a) => ({
      ...a,
      label: a.label ? mask(a.label) : null,
    })),
    sessions: snapshot.sessions
      .filter(
        (s) => !projects?.length || projects.includes(s.cwd ?? "unassigned"),
      )
      .map((s, i) => ({
        ...s,
        title: `Session ${i + 1}`,
        cwd: null,
        logPath: null,
      })),
    billing: snapshot.billing.map((b) => ({
      ...b,
      label: mask(b.label),
      notice: null,
    })),
  };
}
export default function HUDPicture() {
  const [job, setJob] = useState<Job | null>(null);
  const root = useRef<HTMLDivElement>(null),
    started = useRef(false);
  const queries = useRef(new Set<Promise<unknown>>());
  const services = useMemo(
    () => ({
      query: (
        method: "statistics" | "sessions",
        params: Record<string, unknown>,
      ) => {
        const promise = invoke("native_request", { method, params }).then(
          (value) => {
            if (method !== "sessions") return value;
            const response = value as { items: Snapshot["sessions"] };
            return {
              ...response,
              items: response.items.map((session, index) => ({
                ...session,
                title: `Session ${index + 1}`,
                cwd: null,
                logPath: null,
              })),
            };
          },
        );
        queries.current.add(promise);
        void promise
          .finally(() => queries.current.delete(promise))
          .catch(() => {});
        return promise;
      },
    }),
    [],
  );
  const id = new URLSearchParams(location.search).get("pictureId") ?? "";
  useEffect(() => {
    void invoke<Job>("picture_payload", { id })
      .then(setJob)
      .catch((error) =>
        invoke("picture_complete", { id, error: String(error) }),
      );
  }, [id]);
  const screen = job?.presentation.screens.find(
    (s) => s.id === job.params.screen,
  ) ??
    job?.presentation.screens[0] ?? {
      id: "preview",
      monitorIndex: 0,
      hasNotch: false,
    };
  const config = job
    ? getScreenHUDConfig(presentationStore(job.presentation), screen)
    : null;
  const filtered = job && config ? pictureSnapshot(job.snapshot, config) : null;
  const target = job?.params.show ?? "panel";
  if (config) {
    config.animations = false;
    if (target === "hud") {
      config.edge = "top";
      config.offset = 0.5;
    } else if (config.panelLayout.blocks) {
      config.panelLayout = {
        ...config.panelLayout,
        blocks: Object.fromEntries(
          Object.entries(config.panelLayout.blocks).map(([key, block]) => [
            key,
            { ...block, accounts: undefined },
          ]),
        ),
      };
    }
  }
  const metrics =
    config && filtered
      ? stripMetrics(
          config,
          configuredVendors(filtered, config),
          filtered.settings.quotaDisplay,
        )
      : null;
  const width =
    target === "hud"
      ? Math.max(180, (metrics?.width ?? 200) + 80)
      : (config?.panelWidth ?? 540);
  useEffect(() => {
    if (!job || !root.current || started.current) return;
    started.current = true;
    void (async () => {
      try {
        if (
          job.params.agent &&
          config &&
          !configuredVendors(job.snapshot, config).some(
            (vendor) =>
              vendor.key === vendorKey(job.params.agent!.replace(/^api:/, "")),
          )
        )
          throw new Error("The requested agent has no mark on this screen");
        await document.fonts.ready;
        await new Promise((resolve) => setTimeout(resolve, 150));
        await Promise.allSettled([...queries.current]);
        await new Promise((resolve) => setTimeout(resolve, 60));
        const height = Math.min(
          2400,
          Math.max(80, Math.ceil(root.current!.scrollHeight)),
        );
        await invoke("picture_resize", { id, width, height });
        await new Promise((resolve) => setTimeout(resolve, 100));
        const image = await invoke<{
          mimeType: string;
          pngBase64: string;
          width: number;
          height: number;
        }>("native_capture_webview");
        await invoke("picture_complete", {
          id,
          result: {
            ...job.metadata,
            target,
            width,
            height,
            pixelWidth: image.width,
            pixelHeight: image.height,
            identityMasked: true,
            images: [{ mimeType: image.mimeType, data: image.pngBase64 }],
          },
        });
      } catch (error) {
        await invoke("picture_complete", { id, error: String(error) });
      }
    })();
  }, [job, id, width]);
  if (!job || !config || !filtered) return null;
  const light = config.appearance === "light";
  return (
    <FormatProvider
      setting={snapshotLanguage(filtered)}
      quotaDisplay={filtered.settings.quotaDisplay}
    >
      <PanelServicesProvider value={services}>
        <div
          ref={root}
          style={{
            position: "relative",
            width,
            background: light ? "#f5f5f7" : "#161618",
            borderRadius: 24,
            overflow: "hidden",
            colorScheme: light ? "light" : "dark",
          }}
        >
          {target === "hud" ? (
            <HUDScene
              config={config}
              snapshot={{
                ...filtered,
                settings: { ...filtered.settings, animations: false },
              }}
              height={(metrics?.height ?? 24) + 70}
            />
          ) : (
            <>
              <div style={{ position: "relative", padding: 20 }}>
                <HUDGroundProvider look={config}>
                  <SourceHoverPanel
                    config={config}
                    snapshot={filtered}
                    focusVendor={
                      job.params.agent
                        ? vendorKey(job.params.agent.replace(/^api:/, ""))
                        : undefined
                    }
                    light={light}
                    Logo={HUDLogo}
                    active={true}
                  />
                </HUDGroundProvider>
              </div>
            </>
          )}
        </div>
      </PanelServicesProvider>
    </FormatProvider>
  );
}
