import { useEffect, useState } from "react";
import { Notice, Section, SmallButton, type Text } from "./SettingsPrimitives";

/** Extra CODEX_HOME folders, one per signed-in account, read beside the
 * default one. */
export default function CodexAccountHomes({
  t,
  homes,
  onSave,
}: {
  t: Text;
  homes: string[];
  onSave: (homes: string[]) => Promise<unknown>;
}) {
  const saved = homes.join("\n");
  const [text, setText] = useState(saved);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => setText(saved), [saved]);
  const label = t("More Codex accounts", "更多 Codex 账户");

  async function save() {
    setPending(true);
    setError(null);
    const next = [
      ...new Set(
        text
          .split(/\r?\n/)
          .map((path) => path.trim())
          .filter(Boolean),
      ),
    ];
    try {
      await onSave(next);
      setText(next.join("\n"));
    } catch (error) {
      setError(String(error));
    } finally {
      setPending(false);
    }
  }

  return (
    <Section
      title={label}
      footer={t(
        "One CODEX_HOME folder per line. Your default account is always read.",
        "每行一个 CODEX_HOME 文件夹，默认账户总会读取。",
      )}
    >
      <div className="settings-field">
        <textarea
          aria-label={label}
          value={text}
          onChange={(event) => setText(event.target.value)}
          disabled={pending}
          rows={3}
          spellCheck={false}
          placeholder="~/.codex-work"
        />
        <div className="settings-actions">
          <SmallButton
            variant="default"
            disabled={pending || text === saved}
            onClick={() => void save()}
          >
            {pending ? t("Saving…", "正在保存…") : t("Save", "保存")}
          </SmallButton>
        </div>
      </div>
      {error && <Notice tone="error">{error}</Notice>}
    </Section>
  );
}
