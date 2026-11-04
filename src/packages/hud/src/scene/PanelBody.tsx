import type { ReactNode } from "react";
import type {
  IslandEvent,
  RequestDelivery,
  Snapshot,
} from "@agent-hud/protocol";
import { snapshotLanguage } from "@agent-hud/ui/format";
import { InlineEventRow } from "../alerts/InlineEventRow";
import { HUDArtwork as HUDLogo } from "../HUDArtwork";
import { SourceHoverPanel } from "../HoverPanel";
import type { HUDConfig, HUDUpdateNotice } from "../model";
import { RequestDeliveryNotice } from "../permissions/ApprovalContent";

/** The panel's body: the alert stack while an alert is on show, otherwise the
 * usage panel (with an alert inline when it sits there), and below either a
 * request's delivery notice. */
export function PanelBody({
  snapshot,
  maxHeight,
  alertStack,
  inUsagePanel,
  event,
  open,
  config,
  focusVendor,
  light,
  footer,
  update,
  dismissNews,
  delivery,
}: {
  snapshot: Snapshot;
  maxHeight?: number;
  alertStack: ReactNode;
  inUsagePanel: boolean;
  event?: IslandEvent;
  open: boolean;
  config: HUDConfig;
  focusVendor?: string;
  light: boolean;
  /** The footer's buttons, each ending an agent's edit hold first. */
  footer: {
    configure?: () => void;
    stats?: () => void;
    update?: () => void;
  };
  update?: HUDUpdateNotice | null;
  dismissNews: () => void;
  delivery?: RequestDelivery;
}) {
  return (
    <>
      {alertStack && !inUsagePanel ? (
        alertStack
      ) : (
        <SourceHoverPanel
          active={open}
          config={config}
          snapshot={snapshot}
          focusVendor={focusVendor}
          light={light}
          Logo={HUDLogo}
          onConfigure={footer.configure}
          onStats={footer.stats}
          update={update}
          onUpdate={footer.update}
          inlineAlert={
            inUsagePanel && event ? (
              <InlineEventRow
                event={event}
                snapshot={snapshot}
                light={light}
                botLogos={config.botLogos}
                zh={snapshotLanguage(snapshot) === "zh"}
                onOpen={() => {
                  dismissNews();
                  footer.stats?.();
                }}
              />
            ) : undefined
          }
          maxHeight={maxHeight}
        />
      )}
      <RequestDeliveryNotice
        delivery={delivery}
        light={light}
        language={snapshotLanguage(snapshot)}
      />
    </>
  );
}
