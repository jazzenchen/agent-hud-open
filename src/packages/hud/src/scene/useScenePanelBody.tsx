import type { ComponentProps } from "react";
import type { Snapshot } from "@agent-hud/protocol";
import { AlertStack } from "./AlertStack";
import { PanelBody } from "./PanelBody";

export interface UseScenePanelBodyArgs extends Omit<
  ComponentProps<typeof PanelBody>,
  "alertStack" | "snapshot" | "maxHeight"
> {
  alertArgs: ComponentProps<typeof AlertStack>;
  hasAlert: boolean;
}

export function useScenePanelBody({
  alertArgs,
  hasAlert,
  ...panelArgs
}: UseScenePanelBodyArgs) {
  const alertStack = hasAlert ? <AlertStack {...alertArgs} /> : null;

  return (shown: Snapshot, maxHeight?: number) => (
    <PanelBody
      snapshot={shown}
      maxHeight={maxHeight}
      alertStack={alertStack}
      {...panelArgs}
    />
  );
}
