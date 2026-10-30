/* The pieces of a form that opens inside a settings card (add a relay, a
 * relay's type, a model's price): a text box on the same 26 pt scale as a
 * Menu, a row that names one, and the Cancel / submit buttons that end the
 * form. InlineForm holds them, so Return submits it. */
import {
  createContext,
  useContext,
  useId,
  type FocusEvent,
  type KeyboardEvent,
  type ReactNode,
} from "react";
import { Button } from "@agent-hud/ui/components/button";
import { Input } from "@agent-hud/ui/components/input";
import { Row, type Text } from "./SettingsPrimitives";

// The id of the InlineForm a text box or submit button belongs to.
const FormOwner = createContext<string | undefined>(undefined);

/** A form whose rows sit straight in the card, so the card's hairlines divide
 * them like any other rows. The <form> itself is an empty, hidden element
 * after them; the text boxes and the submit button join it by its id, so
 * Return in a field still submits. */
export function InlineForm({
  onSubmit,
  children,
}: {
  onSubmit: () => void;
  children: ReactNode;
}) {
  const id = useId();
  return (
    <FormOwner.Provider value={id}>
      {children}
      <form
        id={id}
        hidden
        onSubmit={(event) => {
          event.preventDefault();
          onSubmit();
        }}
      />
    </FormOwner.Provider>
  );
}

export interface TextInputProps {
  label: string;
  value: string;
  change: (value: string) => void;
  placeholder?: string;
  secret?: boolean;
  decimal?: boolean;
  width?: number | string;
  disabled?: boolean;
  autoFocus?: boolean;
  onBlur?: (event: FocusEvent<HTMLInputElement>) => void;
  onKeyDown?: (event: KeyboardEvent<HTMLInputElement>) => void;
}

/** A compact text box; `label` names it for screen readers. */
export function TextInput({
  label,
  value,
  change,
  placeholder,
  secret = false,
  decimal = false,
  width = 220,
  disabled = false,
  autoFocus = false,
  onBlur,
  onKeyDown,
}: TextInputProps) {
  const form = useContext(FormOwner);
  return (
    <Input
      form={form}
      type={secret ? "password" : "text"}
      autoComplete="off"
      spellCheck={false}
      aria-label={label}
      inputMode={decimal ? "decimal" : undefined}
      value={value}
      placeholder={placeholder}
      disabled={disabled}
      autoFocus={autoFocus}
      onChange={(event) => change(event.target.value)}
      onBlur={onBlur}
      onKeyDown={onKeyDown}
      className="h-[26px] rounded-[6px] px-2 py-0 text-xs md:text-xs"
      style={{ width }}
    />
  );
}

/** A text box on its own row: its name on the left, the box on the right. */
export function FieldRow({
  hint,
  ...input
}: TextInputProps & { hint?: ReactNode }) {
  return (
    <Row title={input.label} subtitle={hint}>
      <TextInput {...input} />
    </Row>
  );
}

/** The last row of a form: an optional link or note on the left, Cancel and
 * the submit button on the right. */
export function FormActions({
  t,
  submit,
  ready,
  pending,
  onCancel,
  start,
}: {
  t: Text;
  submit: string;
  ready: boolean;
  pending: boolean;
  onCancel: () => void;
  start?: ReactNode;
}) {
  const form = useContext(FormOwner);
  return (
    <div className="settings-row">
      <div className="settings-row-main">
        <div className="settings-row-text">
          <div>{start}</div>
        </div>
        <div className="settings-row-control">
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="settings-button"
            disabled={pending}
            onClick={onCancel}
          >
            {t("Cancel", "取消")}
          </Button>
          <Button
            type="submit"
            form={form}
            size="sm"
            className="settings-button"
            disabled={pending || !ready}
          >
            {submit}
          </Button>
        </div>
      </div>
    </div>
  );
}
