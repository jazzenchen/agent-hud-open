import type { Account, AccountProfileEntry } from "@agent-hud/protocol";
import type { Translate } from "@agent-hud/ui/format";
import { Pill, Row } from "./SettingsPrimitives";
import { LinkButton } from "./SettingsControls";

function folderLabel(rawPath: string): string {
  const parts = rawPath.replace(/[\\/]+$/, "").split(/[\\/]/);
  return parts.at(-1) || rawPath;
}

export function DefaultAccountRow({
  t,
  account,
  pending,
  copied,
  onCopy,
}: {
  t: Translate;
  account: Account;
  pending: boolean;
  copied: boolean;
  onCopy: () => void;
}) {
  const title = account.name ?? account.label ?? t("Default account");
  const plan = account.planLabel ?? account.plan;
  return (
    <Row
      title={title}
      tag={plan ?? t("Default")}
      subtitle={account.warning ?? account.notice ?? t("Default account")}
    >
      <Pill tone={account.stale ? "orange" : "green"} dot>
        {account.stale ? t("Stale") : t("Active")}
      </Pill>
      <LinkButton disabled={pending} onClick={onCopy}>
        {copied ? t("Copied") : t("Copy launch command")}
      </LinkButton>
    </Row>
  );
}

export function ManagedProfileRow({
  t,
  profile,
  account,
  pending,
  copied,
  onCopy,
  onRemove,
}: {
  t: Translate;
  profile: AccountProfileEntry;
  account?: Account;
  pending: boolean;
  copied: boolean;
  onCopy: () => void;
  onRemove: () => void;
}) {
  const title =
    account?.name ?? profile.email ?? folderLabel(profile.settingPath);
  const plan = account?.planLabel ?? account?.plan;
  const kindLabel = profile.managed ? t("Managed") : t("Imported");
  const tone = account
    ? account.stale
      ? "orange"
      : "green"
    : profile.ready
      ? "blue"
      : "gray";
  const statusLabel = account
    ? account.stale
      ? t("Stale")
      : t("Active")
    : profile.ready
      ? t("Signed in")
      : t("Not signed in");
  return (
    <Row
      title={title}
      tag={plan ?? kindLabel}
      subtitle={
        account?.warning ??
        (plan ? `${kindLabel} · ${profile.settingPath}` : profile.settingPath)
      }
    >
      <Pill tone={tone} dot={tone === "green" || tone === "orange"}>
        {statusLabel}
      </Pill>
      <LinkButton disabled={pending} onClick={onCopy}>
        {copied ? t("Copied") : t("Copy launch command")}
      </LinkButton>
      <LinkButton tone="destructive" disabled={pending} onClick={onRemove}>
        {t("Remove")}
      </LinkButton>
    </Row>
  );
}
