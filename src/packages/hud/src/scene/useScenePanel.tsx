import { useMemo, type ComponentProps } from "react";
import type { Snapshot } from "@agent-hud/protocol";
import { InspectPanel } from "../inspect/InspectPanel";
import type { ClipEnd } from "../inspect/useClipEnd";
import type { HUDVendor } from "../model";
import type { PanelServices } from "../PanelContext";
import type { ListedSessions } from "../panel/listed-sessions";
import { AlertStack } from "./AlertStack";
import { useMeasured } from "./MeasuringCopy";
import { PanelBody } from "./PanelBody";

type Alerts = ComponentProps<typeof AlertStack>;
type Body = ComponentProps<typeof PanelBody>;

/** The usage panel's body: the alert on show or the clipboard's inspector
 * over the usage panel. The panel on show draws it, and so does a hidden
 * copy that measures its natural height (no `maxHeight`) for its next
 * opening, over the snapshot that copy last drew (`measured`). */
export function useScenePanel({
  snapshot,
  config,
  clip,
  light,
  handoff,
  requests,
  request,
  event,
  news,
  decide,
  openSettings,
  onStats,
  inUsagePanel,
  open,
  phase,
  pointerInside,
  focusedVendor,
  footer,
  update,
  delivery,
  services,
  listed,
}: Omit<Alerts, "botLogos"> &
  Omit<Body, "alertStack" | "dismissNews" | "focusVendor"> & {
    clip: ClipEnd;
    /** The hover's phase: anything but collapsed draws the copy again. */
    phase: string;
    pointerInside: boolean;
    focusedVendor?: HUDVendor;
    services: PanelServices | null;
    listed: ListedSessions;
  }) {
  const eventContent = clip.viewing ? (
    <InspectPanel clip={clip} light={light} handoff={handoff} />
  ) : request || event ? (
    <AlertStack
      requests={requests}
      request={request}
      event={event}
      news={news}
      decide={decide}
      openSettings={openSettings}
      onStats={onStats}
      handoff={handoff}
      light={light}
      snapshot={snapshot}
      botLogos={config.botLogos}
    />
  ) : null;
  const panelBody = (shown: Snapshot, maxHeight?: number) => (
    <PanelBody
      snapshot={shown}
      maxHeight={maxHeight}
      alertStack={eventContent}
      inUsagePanel={inUsagePanel}
      event={event}
      open={open}
      config={config}
      focusVendor={focusedVendor?.name}
      light={light}
      footer={footer}
      update={update}
      dismissNews={news.dismiss}
      delivery={delivery}
    />
  );
  // A pointer move never draws the copy again, and at rest (collapsed, no
  // card out, the pointer away) neither does a data update: it keeps the
  // snapshot it last drew and catches up as the pointer arrives, the panel
  // opens or a card comes out.
  const measuring =
    open || phase !== "collapsed" || pointerInside || Boolean(request || event);
  const configKey = useMemo(() => JSON.stringify(config), [config]);
  // What the copy draws, taken when something it shows changes.
  const measured = useMeasured(snapshot, services, listed, measuring, [
    configKey,
    light,
    open,
    focusedVendor?.name,
    request,
    event,
    news.queue,
    news.ends.view === "tail" ? news.ends.tail : news.ends.view,
    clip.viewing && clip.panel.shape,
    inUsagePanel,
    delivery,
  ]);
  return { panelBody, measured };
}
