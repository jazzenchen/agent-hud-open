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
import {
  Notice,
  Pill,
  Row,
  Section,
  SmallButton,
  type Text,
} from "../SettingsPrimitives";
import { FieldRow } from "../SettingsFields";

const labels: Record<string, [string, string]> = {
  apiKey: ["API key", "API 密钥"],
  baseUrl: ["API endpoint", "API 地址"],
  organization: ["Organization", "组织"],
  project: ["Project", "项目"],
};

export function Connection({
  t,
  source,
  action,
  perform,
  pending,
}: {
  t: Text;
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
      label: labels[key] ? t(...labels[key]) : key,
      secret: key === "apiKey",
      required: key === "apiKey",
      placeholder:
        key === "apiKey"
          ? configured
            ? t(
                "Saved; type a new key to replace it",
                "已保存，输入新密钥可替换",
              )
            : undefined
          : key === "baseUrl"
            ? t("Default", "默认")
            : t("Team plans only", "仅团队套餐需要"),
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
        title={t("Connection", "连接配置")}
        action={
          configured === undefined ? undefined : configured ? (
            <Pill tone="green" dot>
              {t("Saved", "已保存")}
            </Pill>
          ) : (
            <Pill>{t("Not set", "未设置")}</Pill>
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
              {t("Remove", "移除")}
            </Button>
            <Button
              type="submit"
              size="sm"
              className="settings-button"
              disabled={pending || !complete}
            >
              {t("Save", "保存")}
            </Button>
          </Row>
          {error && <Notice tone="error">{error}</Notice>}
        </form>
      </Section>
      <Dialog open={confirming} onOpenChange={setConfirming}>
        <DialogContent className="agents-dialog" showCloseButton={false}>
          <DialogHeader>
            <DialogTitle>
              {t(
                `Remove the saved credentials for ${source.name}?`,
                `移除 ${source.name} 已保存的凭据？`,
              )}
            </DialogTitle>
            <DialogDescription>
              {t(
                "Agent HUD stops reading this account until you add them again.",
                "重新添加之前，Agent HUD 不再读取这个账户。",
              )}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <SmallButton onClick={() => setConfirming(false)}>
              {t("Cancel", "取消")}
            </SmallButton>
            <SmallButton variant="destructive" onClick={remove}>
              {t("Remove", "移除")}
            </SmallButton>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
