// The rows Agent plugins is made of, all on the Settings primitives: a status
// row (pill, one action, a ⋯ menu, a next step when there is one), a plugin's
// switch, and a tool group's switch. Product builds draw their own rows with
// `PluginControl` and `ToolSwitchRow`, so both builds read the same.
import type { ReactNode } from "react";
import { MoreHorizontal } from "lucide-react";
import { Button } from "@agent-hud/ui/components/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@agent-hud/ui/components/dropdown-menu";
import type { RowState, Tone, Words } from "../../lib/agent-plugins";
import {
  LinkButton,
  Notice,
  Pill,
  Row,
  SmallButton,
  ToggleRow,
  Unavailable,
  type PillTone,
  type Text,
} from "../SettingsPrimitives";
import { useBriefly } from "./use-status";

const tones: Record<Tone, PillTone> = {
  working: "green",
  installed: "blue",
  attention: "orange",
  failed: "red",
  off: "gray",
};

/** A row's state as a pill; a running command shows its words in grey. */
export function StatePill({ state, t }: { state: RowState; t: Text }) {
  return (
    <Pill tone={state.busy ? "gray" : tones[state.tone]} dot>
      {t(...(state.busy ?? state.status))}
    </Pill>
  );
}

function CopyLink({ text, t }: { text: string; t: Text }) {
  const [copied, setCopied] = useBriefly<true>(1600);
  return (
    <LinkButton
      onClick={() =>
        void navigator.clipboard
          ?.writeText(text)
          .then(() => setCopied(true))
          .catch(() => {})
      }
    >
      {copied ? t("Copied", "已复制") : t("Copy", "复制")}
    </LinkButton>
  );
}

/** The step left after an action; a command in it is drawn as code with a
 * Copy link. */
function NextStep({
  words,
  command,
  t,
}: {
  words: Words;
  command?: string;
  t: Text;
}) {
  const text = t(...words);
  const at = command ? text.indexOf(command) : -1;
  return (
    <Notice
      tone="info"
      action={command && at >= 0 ? <CopyLink text={command} t={t} /> : null}
    >
      {command && at >= 0 ? (
        <>
          {text.slice(0, at)}
          <code>{command}</code>
          {text.slice(at + command.length)}
        </>
      ) : (
        text
      )}
    </Notice>
  );
}

/** The ⋯ menu; an empty one keeps its place so the pills line up. */
function MoreMenu({
  items,
  disabled,
  onAction,
  t,
}: {
  items: RowState["menu"];
  disabled: boolean;
  onAction: (id: string) => void;
  t: Text;
}) {
  if (!items.length)
    return (
      <Button
        variant="ghost"
        size="icon-xs"
        aria-hidden
        tabIndex={-1}
        disabled
        style={{ visibility: "hidden" }}
      />
    );
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="ghost"
          size="icon-xs"
          disabled={disabled}
          aria-label={t("More", "更多")}
        >
          <MoreHorizontal size={15} />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        {items.map((item) => (
          <DropdownMenuItem
            key={item.id}
            variant={item.destructive ? "destructive" : "default"}
            onClick={() => onAction(item.id)}
          >
            {t(...item.label)}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/** One thing Agent HUD adds to a client: its pill, the one action that
 * matters now, removing in its ⋯ menu, and under it what went wrong and the
 * next step (both, when there are both). `pillReason` explains a pill whose cause is elsewhere. */
export function StatusRow({
  title,
  tag,
  subtitle,
  state,
  error,
  pillReason,
  onAction,
  t,
}: {
  title: ReactNode;
  tag?: ReactNode;
  subtitle?: ReactNode;
  state: RowState;
  error?: string | null;
  pillReason?: string | null;
  onAction: (id: string) => void;
  t: Text;
}) {
  const busy = state.busy !== null;
  const action = state.action;
  const pill = <StatePill state={state} t={t} />;
  const button = action && (
    <SmallButton
      disabled={busy || Boolean(action.unavailable)}
      onClick={() => onAction(action.id)}
    >
      {t(...action.label)}
    </SmallButton>
  );
  return (
    <Row
      title={title}
      tag={tag}
      subtitle={subtitle}
      below={
        error || state.next ? (
          <div style={{ display: "grid", gap: 6 }}>
            {error && <Notice tone="error">{error}</Notice>}
            {state.next && (
              <NextStep
                words={state.next.words}
                command={state.next.command}
                t={t}
              />
            )}
          </div>
        ) : null
      }
    >
      {pillReason ? (
        <Unavailable reason={pillReason}>{pill}</Unavailable>
      ) : (
        pill
      )}
      {button &&
        (action?.unavailable && !busy ? (
          <Unavailable reason={t(...action.unavailable)}>{button}</Unavailable>
        ) : (
          button
        ))}
      <MoreMenu items={state.menu} disabled={busy} onAction={onAction} t={t} />
    </Row>
  );
}

/** One thing a plugin may do, with its switch, under the plugin's row. */
export function PluginControl({
  title,
  description,
  checked,
  onChange,
  unavailable,
  pending = false,
}: {
  title: string;
  /** One short line, only when the title alone could mislead. */
  description?: string;
  checked: boolean;
  onChange: (value: boolean) => void;
  /** Why it cannot change now; the switch is drawn disabled with this as its tip. */
  unavailable?: string | null;
  pending?: boolean;
}) {
  return (
    <ToggleRow
      title={title}
      subtitle={description}
      checked={checked}
      change={onChange}
      disabled={Boolean(unavailable) || pending}
      reason={unavailable ?? undefined}
    />
  );
}

/** A tool group's switch in the Agent tools section, what went wrong with
 * it, and, while it is on, the group's own settings as rows under it (drawn
 * with `PluginControl` or `Row`). Open's is the HUD's; a product build adds
 * its groups'. */
export function ToolSwitchRow({
  title,
  description,
  checked,
  onChange,
  pending = false,
  unavailable,
  error,
  children,
}: {
  title: string;
  /** One short line, only when the title alone could mislead. */
  description?: string;
  checked: boolean;
  onChange: (value: boolean) => void;
  pending?: boolean;
  /** Why it cannot change now; the switch is drawn disabled with this as its tip. */
  unavailable?: string | null;
  error?: string | null;
  children?: ReactNode;
}) {
  return (
    <>
      <ToggleRow
        title={title}
        subtitle={description}
        checked={checked}
        change={onChange}
        disabled={Boolean(unavailable) || pending}
        reason={unavailable ?? undefined}
      />
      {error && <Notice tone="error">{error}</Notice>}
      {checked && children}
    </>
  );
}
