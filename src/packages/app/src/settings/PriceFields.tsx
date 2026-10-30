import { useEffect, useState } from "react";
import type { CustomPrice, PriceRow } from "@agent-hud/protocol";
import { LinkButton, Row, Segments, type Text } from "./SettingsPrimitives";
import { FormActions, InlineForm, TextInput } from "./SettingsFields";

type Rates = Record<"input" | "output" | "cacheRead" | "cacheWrite", string>;
const fields = ["input", "output", "cacheRead", "cacheWrite"] as const;

/** One model's own price per million tokens: empty means unknown, 0 free. */
export function PriceEditor({
  row,
  custom,
  pending,
  t,
  onSave,
  onCancel,
}: {
  row: PriceRow;
  custom: CustomPrice | null;
  pending: boolean;
  t: Text;
  onSave: (price: CustomPrice | null) => void;
  onCancel: () => void;
}) {
  const start = custom ?? row.price;
  const [currency, setCurrency] = useState<"USD" | "CNY">(
    start?.currency === "CNY" ? "CNY" : "USD",
  );
  const [rates, setRates] = useState<Rates>(() => {
    const value = (key: (typeof fields)[number]) => {
      const number = start?.[key];
      return typeof number === "number"
        ? String(Number(number.toFixed(6)))
        : "";
    };
    return {
      input: value("input"),
      output: value("output"),
      cacheRead: value("cacheRead"),
      cacheWrite: value("cacheWrite"),
    };
  });
  const parsed = (text: string) => {
    if (text.trim() === "") return null;
    const number = Number(text);
    return Number.isFinite(number) && number >= 0 ? number : undefined;
  };
  const values = fields.map((key) => parsed(rates[key]));
  const valid =
    values.every((value) => value !== undefined) &&
    values[0] !== null &&
    values[1] !== null;
  const names: Record<(typeof fields)[number], string> = {
    input: t("Input", "输入"),
    output: t("Output", "输出"),
    cacheRead: t("Cache read", "缓存读"),
    cacheWrite: t("Cache write", "缓存写"),
  };
  // The four rates side by side, each named above its box.
  const grid = (
    <div
      style={{
        display: "grid",
        gridTemplateColumns: "repeat(4, minmax(0, 1fr))",
        gap: 8,
      }}
    >
      {fields.map((key) => (
        <label key={key} style={{ display: "grid", gap: 4 }}>
          <span className="settings-row-subtitle">{names[key]}</span>
          <TextInput
            label={names[key]}
            value={rates[key]}
            change={(text) => setRates({ ...rates, [key]: text })}
            placeholder={key.startsWith("cache") ? t("= input", "同输入") : ""}
            decimal
            width="100%"
          />
        </label>
      ))}
    </div>
  );
  return (
    <InlineForm
      onSubmit={() => {
        if (!valid || pending) return;
        onSave({
          currency,
          input: values[0] as number,
          output: values[1] as number,
          cacheRead: values[2] ?? null,
          cacheWrite: values[3] ?? null,
        });
      }}
    >
      <Row
        title={t("Price per million tokens", "每百万 Token 价格")}
        subtitle={t(
          "0 is free; an empty cache rate uses the input's.",
          "0 表示免费；缓存价留空按输入价。",
        )}
        below={grid}
      >
        <Segments
          label={t("Currency", "币种")}
          value={currency}
          options={[
            ["USD", "USD"],
            ["CNY", "CNY"],
          ]}
          change={(value) => setCurrency(value as "USD" | "CNY")}
        />
      </Row>
      <FormActions
        t={t}
        pending={pending}
        ready={valid}
        submit={t("Save", "保存")}
        onCancel={onCancel}
        start={
          custom && (
            <LinkButton disabled={pending} onClick={() => onSave(null)}>
              {t("Use the automatic price", "改回自动价格")}
            </LinkButton>
          )
        }
      />
    </InlineForm>
  );
}

/** A relay's multiplier, kept when the field is left or Return is pressed. */
export function MultiplierInput({
  label,
  value,
  disabled,
  placeholder,
  onCommit,
}: {
  label: string;
  value: number | undefined;
  disabled: boolean;
  placeholder: string;
  onCommit: (times: number | null) => void;
}) {
  const [text, setText] = useState(value === undefined ? "" : String(value));
  useEffect(() => setText(value === undefined ? "" : String(value)), [value]);
  const commit = () => {
    const trimmed = text.trim();
    if (trimmed === "") {
      if (value !== undefined) onCommit(null);
      return;
    }
    const times = Number(trimmed);
    if (Number.isFinite(times) && times > 0 && times <= 100 && times !== value)
      onCommit(times);
    else if (!(Number.isFinite(times) && times > 0 && times <= 100))
      setText(value === undefined ? "" : String(value));
  };
  return (
    <>
      <span aria-hidden>×</span>
      <TextInput
        label={label}
        value={text}
        change={setText}
        disabled={disabled}
        placeholder={placeholder}
        decimal
        width={64}
        onBlur={commit}
        onKeyDown={(event) => {
          if (event.key === "Enter") commit();
        }}
      />
    </>
  );
}
