import { useEffect, useRef, useState, type ReactNode } from "react";
import type { NativeMethod } from "@agent-hud/protocol";
import {
  Notice,
  Section,
  SmallButton,
  Unavailable,
  type Text,
} from "./SettingsPrimitives";

interface CliStatus {
  installable: boolean;
  defaultSettings: string;
  notice?: string | null;
}

/** The Antigravity CLI settings files Agent HUD hooks into to read each
 * signed-in account's quota. */
export default function AntigravityAccounts({
  t,
  settings,
  action,
}: {
  t: Text;
  settings: string[];
  action: (
    method: NativeMethod,
    params?: Record<string, unknown>,
  ) => Promise<unknown>;
}) {
  const saved = settings.join("\n");
  const [text, setText] = useState(saved);
  const [status, setStatus] = useState<CliStatus | null>(null);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const latestAction = useRef(action);
  latestAction.current = action;
  useEffect(() => setText(saved), [saved]);
  useEffect(() => {
    let active = true;
    void latestAction.current("antigravity_cli_status").then(
      (value) => {
        if (active) setStatus(value as CliStatus);
      },
      (error) => {
        if (active) setError(String(error));
      },
    );
    return () => {
      active = false;
    };
  }, []);
  const paths = () => [
    ...new Set(
      text
        .split(/\r?\n/)
        .map((path) => path.trim())
        .filter(Boolean),
    ),
  ];
  const label = t("Antigravity CLI accounts", "Antigravity CLI 账户");
  // Why the field is off: still checking, or the CLI can't take the hook.
  const reason = !status
    ? error
      ? undefined
      : t("Checking the Antigravity CLI…", "正在检查 Antigravity CLI…")
    : !status.installable
      ? (status.notice ??
        t(
          "The Antigravity CLI can't be connected.",
          "无法连接 Antigravity CLI。",
        ))
      : undefined;
  const ready = !!status?.installable;

  async function save() {
    setPending(true);
    setError(null);
    try {
      await action("set_setting", {
        key: "antigravityCliSettings",
        value: paths(),
      });
    } catch (error) {
      setError(String(error));
    } finally {
      setPending(false);
    }
  }

  const gate = (button: ReactNode, off: boolean) =>
    off && reason ? (
      <Unavailable reason={reason}>{button}</Unavailable>
    ) : (
      button
    );
  const addDefaultOff =
    pending || !ready || paths().includes(status?.defaultSettings ?? "");
  const connectOff = pending || !ready || text === saved;

  return (
    <Section
      title={label}
      footer={t(
        "One settings.json per line. Restart the CLI after connecting; removing a path undoes the change.",
        "每行一个 settings.json。连接后重启 CLI；移除路径即可还原。",
      )}
    >
      <div className="settings-field">
        <textarea
          aria-label={label}
          value={text}
          onChange={(event) => setText(event.target.value)}
          disabled={pending || !ready}
          rows={3}
          spellCheck={false}
          placeholder="~/.gemini/antigravity-cli/settings.json"
        />
        <div className="settings-actions">
          {gate(
            <SmallButton
              disabled={addDefaultOff}
              onClick={() =>
                setText([...paths(), status!.defaultSettings].join("\n"))
              }
            >
              {t("Add default", "添加默认配置")}
            </SmallButton>,
            addDefaultOff,
          )}
          {gate(
            <SmallButton
              variant="default"
              disabled={connectOff}
              onClick={() => void save()}
            >
              {pending ? t("Saving…", "正在保存…") : t("Connect", "连接")}
            </SmallButton>,
            connectOff,
          )}
        </div>
      </div>
      {status?.notice && <Notice>{status.notice}</Notice>}
      {error && <Notice tone="error">{error}</Notice>}
    </Section>
  );
}
