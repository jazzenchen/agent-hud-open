/* Consent gate for reading GitHub Copilot quota from an existing GitHub
 * sign-in on this computer. Turning it on asks each time; turning it off
 * takes effect at once. */
import { useState } from "react";
import type { NativeMethod, Snapshot } from "@agent-hud/protocol";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@agent-hud/ui/components/dialog";
import type { Translate } from "@agent-hud/ui/format";
import { SmallButton, ToggleRow } from "../SettingsControls";
import { Section } from "../SettingsPrimitives";
import {
  copilotConsentBody,
  copilotConsentPlatform,
  copilotQuotaToggle,
} from "./copilot-quota";
import type { Perform } from "./use-perform";

export function CopilotQuota({
  t,
  snapshot,
  action,
  perform,
  pending,
}: {
  t: Translate;
  snapshot: Snapshot;
  action: (
    method: NativeMethod,
    params?: Record<string, unknown>,
  ) => Promise<unknown>;
  perform: Perform;
  pending: boolean;
}) {
  const [confirming, setConfirming] = useState(false);
  const enabled = snapshot.settings.readCopilotQuota === true;
  const platform = copilotConsentPlatform(snapshot.connection);

  const save = (value: boolean) =>
    void perform(() =>
      action("set_setting", { key: "readCopilotQuota", value }),
    );

  const toggle = (value: boolean) => {
    const step = copilotQuotaToggle(enabled, value);
    if (step === "confirm") setConfirming(true);
    else if (step === "disable") save(false);
  };

  const allow = () => {
    if (pending) return;
    setConfirming(false);
    save(true);
  };

  return (
    <>
      <Section>
        <ToggleRow
          title={t("Read GitHub Copilot quota")}
          subtitle={t("Uses the GitHub sign-in on this computer.")}
          checked={enabled || confirming}
          disabled={pending}
          change={toggle}
        />
      </Section>
      <Dialog open={confirming} onOpenChange={setConfirming}>
        <DialogContent className="agents-dialog" showCloseButton={false}>
          <DialogHeader>
            <DialogTitle>{t("Read GitHub Copilot quota?")}</DialogTitle>
            <DialogDescription>
              {copilotConsentBody(platform, t)}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <SmallButton onClick={() => setConfirming(false)}>
              {t("Cancel")}
            </SmallButton>
            <SmallButton variant="default" disabled={pending} onClick={allow}>
              {t("Allow")}
            </SmallButton>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
