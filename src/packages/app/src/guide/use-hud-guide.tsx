// The guide's window beside the HUD, run by the main screen's HUD alone:
// decided once at launch, without waiting for Settings, opened again from
// Settings' menu (`guide:open`), and handing the rest of the tour to
// Settings (the record's step names a Settings row).
import { useEffect, useRef, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { emit, listen } from "@tauri-apps/api/event";
import type { HUDGuide } from "@agent-hud/hud/GuideDock";
import type { HUDConfig } from "@agent-hud/hud/model";
import type { GuideRecord, NativeMethod, Snapshot } from "@agent-hud/protocol";
import { snapshotLanguage } from "@agent-hud/ui/format";
import { modePatch } from "../lib/mode-patch";
import { HUDGuideCard } from "./HUDGuideCard";
import {
  atStep,
  closed,
  guideAtLaunch,
  isHUDStep,
  latestNews,
  projectPage,
  starAnswered,
  tourOrder,
  type GuideTarget,
  type GuideView,
  type NewsEntry,
  type SettingsStep,
} from "./plan";

/** How long the panel stays open before the first step counts: a pointer
 * passing over the HUD does not. */
const lookMs = 1000;

/** Settings' way to the HUD's guide window, and to a place in Settings. */
export const guideEvents = { open: "guide:open", spotlight: "guide:spotlight" };

export function useHUDGuide({
  active,
  snapshot,
  config,
  update,
  action,
  demo,
  screens,
  news,
  steps,
}: {
  /** This is the main screen's HUD, which runs the guide. */
  active: boolean;
  snapshot: Snapshot;
  config: HUDConfig;
  update: (patch: Partial<HUDConfig>) => unknown;
  action: (
    method: NativeMethod,
    params?: Record<string, unknown>,
  ) => Promise<unknown>;
  demo: boolean;
  screens: number;
  news: readonly NewsEntry[];
  steps: readonly SettingsStep[];
}): HUDGuide | null {
  const record = snapshot.settings.guide;
  const chinese = snapshotLanguage(snapshot) === "zh";
  const t = (en: string, zh: string) => (chinese ? zh : en);
  // The guide's arrow and drag picture move as the HUD does.
  const moving =
    snapshot.settings.animations !== false &&
    config.animations !== false &&
    !window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  const [version, setVersion] = useState<string | null>(
    isTauri() ? null : "0.0.0",
  );
  const [view, setView] = useState<GuideView | null>(null);
  const [looked, setLooked] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  // What this window last wrote, until the snapshot brings it back.
  const written = useRef<GuideRecord | null>(null);
  const current = () => written.current ?? record;
  const write = (next: GuideRecord) => {
    written.current = next;
    void action("set_setting", { key: "guide", value: next }).catch((error) =>
      console.warn("The guide's record could not be saved", error),
    );
  };
  useEffect(() => {
    if (isTauri()) void getVersion().then(setVersion);
  }, []);
  useEffect(() => {
    const mine = written.current;
    if (
      mine &&
      record &&
      (["since", "seen", "step", "star"] as const).every(
        (key) => mine[key] === record[key],
      )
    )
      written.current = null;
  }, [record]);

  const order = tourOrder(steps);
  const open = (next: GuideView | null) => {
    setView(next);
    setLooked(false);
    const base = current();
    if (next?.kind === "tour" && base && base.step !== next.step)
      write(atStep(base, next.step));
  };

  // Once, when the record and the version are both known; the demo opens
  // no guide by itself.
  const decided = useRef(false);
  useEffect(() => {
    if (!active || decided.current || !record || !version) return;
    decided.current = true;
    if (demo) return;
    const view = guideAtLaunch(record, version, news, Date.now());
    // A hidden HUD shows no window: the tour starts on its Settings rows.
    if (
      view?.kind === "tour" &&
      isHUDStep(view.step) &&
      snapshot.settings.hudVisible === false
    )
      return write(atStep(record, steps[0]?.id ?? "star"));
    open(view);
  }, [active, record, version, demo]);

  // Settings moved the tour on, closed it, or stepped back to a step here.
  useEffect(() => {
    if (!active || !decided.current || !record) return;
    if (view?.kind === "news" || record.step === (view?.step ?? "")) return;
    if (record.step && isHUDStep(record.step))
      setView({ kind: "tour", step: record.step });
    else if (view) setView(null);
  }, [record?.step]);

  const reopen = useRef<(kind: "tour" | "news") => void>(() => {});
  reopen.current = (kind) => {
    const base = current();
    if (!base || !version) return;
    open(
      kind === "tour"
        ? { kind: "tour", step: "hud" }
        : latestNews(base, version, news, Date.now()),
    );
  };
  useEffect(() => {
    if (!active || !isTauri()) return;
    const subscription = listen<"tour" | "news">(
      guideEvents.open,
      ({ payload }) => reopen.current(payload),
    );
    return () => void subscription.then((unlisten) => unlisten());
  }, [active]);

  if (!active || !view) return null;
  const toSettings = (target: GuideTarget) => {
    void invoke("hud_show_main", { page: target.page ?? null });
    if (target.anchor) {
      // Settings may be opening: it also reads the place when it starts.
      try {
        localStorage.setItem(
          guideEvents.spotlight,
          JSON.stringify({ ...target, at: Date.now() }),
        );
      } catch {}
      void emit(guideEvents.spotlight, target);
    }
  };
  const close = () => {
    const base = current();
    if (base && version) write(closed(base, version));
    setView(null);
  };
  const firstInSettings = steps[0];
  const next = () => {
    const base = current();
    if (view.kind !== "tour" || !base) return;
    if (view.step === "hud") return open({ kind: "tour", step: "mode" });
    // The tour goes on in Settings, on its first row.
    const step = firstInSettings?.id ?? "star";
    write(atStep(base, step));
    void invoke("hud_show_main", { page: firstInSettings?.page ?? null });
    setView(null);
  };
  return {
    onPanel: (panelOpen) => {
      if (view.kind !== "tour" || view.step !== "hud") return;
      if (timer.current) clearTimeout(timer.current);
      timer.current = panelOpen
        ? setTimeout(() => setLooked(true), lookMs)
        : null;
    },
    content: (
      <HUDGuideCard
        view={view}
        looked={looked}
        place={config}
        screens={screens}
        moving={moving}
        count={[
          order.indexOf(view.kind === "tour" ? view.step : "") + 1,
          order.length,
        ]}
        t={t}
        next={next}
        back={() => open({ kind: "tour", step: "hud" })}
        close={close}
        resume={() => {
          const step = steps.find((item) => item.id === record?.step);
          void invoke("hud_show_main", { page: step?.page ?? null });
          setView(null);
        }}
        show={toSettings}
        star={(openPage) => {
          const base = current();
          if (openPage)
            void action("open_link", { url: projectPage }).catch(
              () => undefined,
            );
          if (base) write(starAnswered(base));
          if (view.kind === "news") setView({ ...view, star: false });
        }}
        setMode={(mode) => update(modePatch(config, mode))}
      />
    ),
  };
}
