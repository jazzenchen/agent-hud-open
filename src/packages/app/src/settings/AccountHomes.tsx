import { useEffect, useState } from "react";
import { Notice, Section, SmallButton, type Text } from "./SettingsPrimitives";
import { usePending } from "./use-pending";

/** Extra client folders (CODEX_HOME, CLAUDE_CONFIG_DIR), one per signed-in
 * account, read beside the default one. */
export default function AccountHomes({
  t,
  label,
  variable,
  placeholder,
  homes,
  onSave,
}: {
  t: Text;
  label: string;
  /** The variable that names the client's folder. */
  variable: string;
  placeholder: string;
  homes: string[];
  onSave: (homes: string[]) => Promise<unknown>;
}) {
  const saved = homes.join("\n");
  const [text, setText] = useState(saved);
  const { pending, error, run } = usePending();
  useEffect(() => setText(saved), [saved]);

  async function save() {
    const next = [
      ...new Set(
        text
          .split(/\r?\n/)
          .map((path) => path.trim())
          .filter(Boolean),
      ),
    ];
    if (await run(() => onSave(next))) setText(next.join("\n"));
  }

  return (
    <Section
      title={label}
      footer={t(
        `One ${variable} folder per line. Your default account is always read.`,
        `每行一个 ${variable} 文件夹，默认账户总会读取。`,
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
          placeholder={placeholder}
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
