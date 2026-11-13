// The guide in Settings. It follows the record: while the tour's step is a
// Settings row (or the Star), it opens that row's page and shows the card on
// it. It also rings the place a news item's "Show me" names, and starts the
// tour here when the HUD is hidden and cannot.
import { useEffect, useRef, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { GuideRecord, Snapshot } from "@agent-hud/protocol";
import {
  useText,
  type SettingsPaneProps,
} from "../settings/SettingsPrimitives";
import { guideEvents } from "./use-hud-guide";
import {
  atStep,
  closed,
  isHUDStep,
  nextStep,
  previousStep,
  projectPage,
  starAnswered,
  starStep,
  tourOrder,
  type GuideTarget,
  type SettingsStep,
} from "./plan";
import { SettingsGuideCard } from "./SettingsGuideCard";
import { Ring, useAnchorBox } from "./Spotlight";
import "./guide.css";

/** A "Show me" sent while Settings was opening, taken when it starts. */
function pendingSpotlight(): GuideTarget | null {
  try {
    const value = JSON.parse(
      localStorage.getItem(guideEvents.spotlight) ?? "null",
    );
    localStorage.removeItem(guideEvents.spotlight);
    return value && Date.now() - value.at < 15_000 ? value : null;
  } catch {
    return null;
  }
}

export function GuideLayer({
  snapshot,
  settingsProps,
  version,
  steps,
  setPage,
}: {
  snapshot: Snapshot | null;
  settingsProps: SettingsPaneProps | null;
  version: string | null;
  steps: readonly SettingsStep[];
  setPage: (page: string) => void;
}) {
  const t = useText(snapshot);
  const record = snapshot?.settings.guide;
  const stepId = record?.step ?? "";
  const step = steps.find((item) => item.id === stepId) ?? null;
  const touring = Boolean(step) || stepId === starStep;
  const [ring, setRing] = useState<string | null>(null);
  const box = useAnchorBox(step?.anchor ?? ring);
  const action = settingsProps?.action;
  const write = (next: GuideRecord) =>
    void action?.("set_setting", { key: "guide", value: next }).catch((error) =>
      console.warn("The guide's record could not be saved", error),
    );

  // Each step opens its row's page once; the person may wander off after.
  useEffect(() => {
    if (step) setPage(step.page);
  }, [step?.id]);

  // "Show me" from the news beside the HUD: the place rings for a while.
  const show = (target: GuideTarget | null) => {
    if (!target) return;
    if (target.page) setPage(target.page);
    setRing(target.anchor ?? null);
  };
  useEffect(() => {
    if (!ring) return;
    const timer = setTimeout(() => setRing(null), 6000);
    return () => clearTimeout(timer);
  }, [ring]);
  const latest = useRef({ snapshot, record, steps, write });
  latest.current = { snapshot, record, steps, write };
  useEffect(() => {
    if (!isTauri()) return;
    show(pendingSpotlight());
    const spotlight = listen<GuideTarget>(
      guideEvents.spotlight,
      ({ payload }) => {
        pendingSpotlight();
        show(payload);
      },
    );
    // The HUD runs the tour's first steps; a hidden HUD cannot, so the tour
    // starts on its first Settings row.
    const open = listen<"tour" | "news">(guideEvents.open, ({ payload }) => {
      const now = latest.current;
      const first = now.steps[0]?.id ?? starStep;
      if (
        payload === "tour" &&
        now.record &&
        now.snapshot?.settings.hudVisible === false
      )
        now.write(atStep(now.record, first));
    });
    return () => {
      void spotlight.then((unlisten) => unlisten());
      void open.then((unlisten) => unlisten());
    };
  }, []);

  if (!snapshot || !record) return null;
  if (!touring || isHUDStep(stepId))
    return ring && box ? <Ring box={box} /> : null;
  const order = tourOrder(steps);
  const close = () => {
    if (version) write(closed(record, version));
  };
  return (
    <SettingsGuideCard
      step={step}
      box={box}
      snapshot={snapshot}
      count={[order.indexOf(stepId) + 1, order.length]}
      t={t}
      next={() => {
        const following = nextStep(stepId, order);
        if (following) write(atStep(record, following));
        else close();
      }}
      back={() => {
        const before = previousStep(stepId, order);
        if (before) write(atStep(record, before));
      }}
      close={close}
      star={() => {
        void action?.("open_link", { url: projectPage }).catch(() => undefined);
        if (version) write(closed(starAnswered(record), version));
      }}
    />
  );
}
