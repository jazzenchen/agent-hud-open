/* The API credentials Agent HUD reads an agent's account with, for the agents
 * whose reader asks for them (DeepSeek, Kimi, GLM, OpenCode). */
import { useState } from "react";
import type { NativeMethod, Provider } from "@agent-hud/protocol";
import { Button } from "@agent-hud/ui/components/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@agent-hud/ui/components/dialog";
import { Notice, Pill, Row, Section } from "../SettingsPrimitives";
import type { Translate } from "@agent-hud/ui/format";
import { SmallButton } from "../SettingsControls";
import { FieldRow } from "../SettingsFields";

const labels: Record<string, (t: Translate) => string> = {
  apiKey: (t) => t("API key"),
  baseUrl: (t) => t("API endpoint"),
  organization: (t) => t("Organization"),
  project: (t) => t("Project"),
};

export function Connection({
  t,
  source,
  action,
  perform,
  pending,
}: {
  t: Translate;
  source: Provider;
  action: (
    method: NativeMethod,
    params?: Record<string, unknown>,
  ) => Promise<unknown>;
  perform: (
    job: () => Promise<unknown>,
    report?: (message: string) => void,
  ) => Promise<boolean>;
  pending: boolean;
}) {
  const [credentials, setCredentials] = useState<Record<string, string>>({});
  const [confirming, setConfirming] = useState(false);
  // A failed Save or Remove shows here, under the buttons: this section is the
  // last on the page, so the detail's own notice at the top can be out of view.
  const [error, setError] = useState<string | null>(null);
  const configured = source.accountStatus?.configured;
  const fields = (source.accountStatus?.configurableFields ?? []).map(
    (key) => ({
      key,
      label: labels[key]?.(t) ?? key,
      secret: key === "apiKey",
      required: key === "apiKey",
      placeholder:
        key === "apiKey"
          ? configured
            ? t("Saved; type a new key to replace it")
            : undefined
          : key === "baseUrl"
            ? t("Default")
            : t("Team plans only"),
    }),
  );
  if (!fields.length) return null;
  const complete = fields.every(
    (field) => !field.required || credentials[field.key]?.trim(),
  );
  const save = () => {
    if (pending || !complete) return;
    setError(null);
    void perform(
      () => action("set_credentials", { provider: source.id, credentials }),
      setError,
    ).then((saved) => {
      if (saved) setCredentials({});
    });
  };
  const remove = () => {
    setConfirming(false);
    setError(null);
    void perform(
      () => action("remove_credentials", { provider: source.id }),
      setError,
    );
  };

  return (
    <>
      <Section
        title={t("Connection")}
        action={
          configured === undefined ? undefined : configured ? (
            <Pill tone="green" dot>
              {t("Saved")}
            </Pill>
          ) : (
            <Pill>{t("Not set")}</Pill>
          )
        }
        plain
      >
        {/* The form is the card, so its rows keep the card's hairlines and
         * Return in any field saves. */}
        <form
          className="settings-card"
          onSubmit={(event) => {
            event.preventDefault();
            save();
          }}
        >
          {fields.map((field) => (
            <FieldRow
              key={field.key}
              label={field.label}
              value={credentials[field.key] ?? ""}
              change={(value) =>
                setCredentials((previous) => ({
                  ...previous,
                  [field.key]: value,
                }))
              }
              placeholder={field.placeholder}
              secret={field.secret}
              width={240}
            />
          ))}
          <Row title="">
            {/* Remove stays on even when nothing reads as saved: a saved file
             * the engine cannot read reports as not set, and only Remove
             * deletes it. The dialog guards it. */}
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="settings-button"
              disabled={pending}
              onClick={() => setConfirming(true)}
            >
              {t("Remove")}
            </Button>
            <Button
              type="submit"
              size="sm"
              className="settings-button"
              disabled={pending || !complete}
            >
              {t("Save")}
            </Button>
          </Row>
          {error && <Notice tone="error">{error}</Notice>}
        </form>
      </Section>
      <Dialog open={confirming} onOpenChange={setConfirming}>
        <DialogContent className="agents-dialog" showCloseButton={false}>
          <DialogHeader>
            <DialogTitle>
              {t("Remove the saved credentials for {{name}}?", {
                name: source.name,
              })}
            </DialogTitle>
            <DialogDescription>
              {t(
                "Agent HUD stops reading this account until you add them again.",
              )}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <SmallButton onClick={() => setConfirming(false)}>
              {t("Cancel")}
            </SmallButton>
            <SmallButton variant="destructive" onClick={remove}>
              {t("Remove")}
            </SmallButton>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
