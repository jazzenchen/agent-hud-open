import { useEffect, useRef, useState } from "react";
import type {
  Account,
  AccountProfileEntry,
  AccountProfileLaunchCommand,
  AccountProfileSignInStatus,
  NativeMethod,
  Snapshot,
} from "@agent-hud/protocol";
import type { Translate } from "@agent-hud/ui/format";
import {
  DefaultAccountRow,
  ManagedProfileRow,
} from "./ManagedAccountProfileRows";
import { Collapsible, Notice, Row, Section } from "./SettingsPrimitives";
import { LinkButton, SmallButton } from "./SettingsControls";
import { usePending } from "./use-pending";

type ManagedProvider = "antigravity" | "claude" | "codex";
type HomesKey = "antigravityHomes" | "claudeHomes" | "codexHomes";

const PLACEHOLDERS: Record<ManagedProvider, string> = {
  antigravity: "~/.gemini-work",
  claude: "~/.claude-work",
  codex: "~/.codex-work",
};

export default function ManagedAccountProfiles({
  t,
  provider,
  settingKey,
  homes,
  snapshot,
  action,
}: {
  t: Translate;
  provider: ManagedProvider;
  settingKey: HomesKey;
  homes: string[];
  snapshot: Snapshot;
  action: (
    method: NativeMethod,
    params?: Record<string, unknown>,
  ) => Promise<unknown>;
}) {
  const [status, setStatus] = useState<AccountProfileSignInStatus | null>(null);
  const [importPath, setImportPath] = useState("");
  const [copiedKey, setCopiedKey] = useState<string | null>(null);
  const { pending, error, setError, run } = usePending();
  const latestAction = useRef(action);
  latestAction.current = action;
  const homesSig = homes.join("\n");

  useEffect(() => {
    let alive = true;
    void latestAction
      .current("account_profile_sign_in_status", { provider, homes })
      .then(
        (value) => {
          if (alive) setStatus(value as AccountProfileSignInStatus);
        },
        (err) => {
          if (alive) setError(String(err));
        },
      );
    return () => {
      alive = false;
    };
  }, [provider, homesSig]);

  const signingIn = status?.state === "signing_in";

  useEffect(() => {
    if (!signingIn) return;
    let alive = true;
    const timer = setInterval(() => {
      void latestAction
        .current("account_profile_sign_in_status", { provider })
        .then((value) => {
          if (!alive) return;
          const next = value as AccountProfileSignInStatus;
          setStatus(next);
          if (next.added) {
            void latestAction.current("refresh");
          }
        })
        .catch(() => {});
    }, 1000);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, [provider, signingIn]);

  const accounts = (snapshot.accounts ?? []).filter(
    (acct) => acct.provider === provider,
  );
  const profiles: AccountProfileEntry[] =
    status?.profiles ??
    homes.map((settingPath) => ({
      settingPath,
      managed: false,
      ready: false,
      email: null,
    }));

  const matchedAccountIds = new Set<string>();
  const profileCards = profiles.map((profile) => {
    const emailLower = profile.email?.toLowerCase();
    const matched = emailLower
      ? accounts.find(
          (acct) =>
            acct.label?.toLowerCase() === emailLower &&
            !matchedAccountIds.has(acct.id),
        )
      : undefined;
    if (matched) matchedAccountIds.add(matched.id);
    return { profile, account: matched };
  });
  const defaultAccounts: Account[] = accounts.filter(
    (acct) => !matchedAccountIds.has(acct.id),
  );

  async function startSignIn() {
    await run(async () => {
      const res = await action("account_profile_sign_in_start", { provider });
      setStatus(res as AccountProfileSignInStatus);
    });
  }

  async function cancelSignIn() {
    await run(async () => {
      const res = await action("account_profile_sign_in_cancel", { provider });
      setStatus(res as AccountProfileSignInStatus);
    });
  }

  async function addImportedFolder() {
    const trimmed = importPath.trim();
    if (!trimmed) return;
    const next = [...new Set([...homes, trimmed])];
    if (
      await run(() => action("set_setting", { key: settingKey, value: next }))
    )
      setImportPath("");
  }

  async function removeFolder(targetPath: string) {
    const next = homes.filter((item) => item !== targetPath);
    await run(() => action("set_setting", { key: settingKey, value: next }));
  }

  async function copyCommand(key: string, home?: string) {
    setError(null);
    try {
      const res = (await action("account_profile_launch_command", {
        provider,
        home: home ?? null,
      })) as AccountProfileLaunchCommand;
      await navigator.clipboard.writeText(res.command);
      setCopiedKey(key);
      setTimeout(() => {
        setCopiedKey((prev) => (prev === key ? null : prev));
      }, 1600);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  return (
    <>
      <Section
        title={t("Managed accounts")}
        footer={t(
          "Sign in isolated subscription accounts with the official CLI without touching your system keychain or default login.",
        )}
        action={
          signingIn ? (
            <SmallButton disabled={pending} onClick={() => void cancelSignIn()}>
              {t("Cancel sign-in")}
            </SmallButton>
          ) : (
            <SmallButton
              variant="default"
              disabled={pending}
              onClick={() => void startSignIn()}
            >
              {t("+ Sign in new account")}
            </SmallButton>
          )
        }
      >
        {defaultAccounts.map((acct) => {
          const key = `default:${acct.id}`;
          return (
            <DefaultAccountRow
              key={key}
              t={t}
              account={acct}
              pending={pending}
              copied={copiedKey === key}
              onCopy={() => void copyCommand(key)}
            />
          );
        })}
        {profileCards.map(({ profile, account }) => {
          const key = `home:${profile.settingPath}`;
          return (
            <ManagedProfileRow
              key={key}
              t={t}
              profile={profile}
              account={account}
              pending={pending}
              copied={copiedKey === key}
              onCopy={() => void copyCommand(key, profile.settingPath)}
              onRemove={() => void removeFolder(profile.settingPath)}
            />
          );
        })}
        {defaultAccounts.length === 0 && profileCards.length === 0 && (
          <Row
            muted
            title={t("No extra accounts added")}
            subtitle={t(
              "Use + Sign in new account to authorize an isolated subscription in your browser.",
            )}
          />
        )}
      </Section>

      {signingIn && (
        <Notice
          tone="info"
          action={
            status?.authUrl ? (
              <LinkButton
                onClick={() =>
                  void action("open_link", { url: status.authUrl })
                }
              >
                {t("Open sign-in link")}
              </LinkButton>
            ) : undefined
          }
        >
          {t(
            "Complete the official sign-in in your browser. Quota windows will refresh automatically once authorized.",
          )}
        </Notice>
      )}
      {status?.state === "failed" && status.error && (
        <Notice tone="error">{status.error}</Notice>
      )}
      {error && <Notice tone="error">{error}</Notice>}

      <Collapsible
        title={t("Import existing folder")}
        subtitle={t("Bind an isolated folder you already signed in to.")}
      >
        <div className="settings-field">
          <input
            type="text"
            aria-label={t("Import existing folder")}
            value={importPath}
            onChange={(event) => setImportPath(event.target.value)}
            disabled={pending}
            spellCheck={false}
            placeholder={PLACEHOLDERS[provider]}
          />
          <div className="settings-actions">
            <SmallButton
              variant="default"
              disabled={pending || !importPath.trim()}
              onClick={() => void addImportedFolder()}
            >
              {pending ? t("Saving…") : t("Import folder")}
            </SmallButton>
          </div>
        </div>
      </Collapsible>
    </>
  );
}
