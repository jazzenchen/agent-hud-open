import { useState } from "react";
import type { RelayCustom, RelayKind } from "@agent-hud/protocol";
import { Menu, Row, Segments, type Text } from "./SettingsPrimitives";
import { FieldRow, FormActions, InlineForm } from "./SettingsFields";

/** A relay added by hand: its address and key, and a name if the owner wants
 * one. What kind of relay it is is found out when it is read. */
export function RelayForm({
  t,
  pending,
  onSave,
  onCancel,
}: {
  t: Text;
  pending: boolean;
  onSave: (relay: Record<string, unknown>) => void;
  onCancel: () => void;
}) {
  const [name, setName] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [key, setKey] = useState("");
  return (
    <InlineForm onSubmit={() => onSave({ name, baseUrl, key })}>
      <FieldRow
        label={t("Address", "地址")}
        value={baseUrl}
        change={setBaseUrl}
        placeholder="https://relay.example.com"
        autoFocus
      />
      <FieldRow
        label={t("API key", "API Key")}
        value={key}
        change={setKey}
        secret
      />
      <FieldRow
        label={t("Name", "名称")}
        value={name}
        change={setName}
        placeholder={t("Optional", "可不填")}
      />
      <FormActions
        t={t}
        pending={pending}
        ready={Boolean(baseUrl.trim() && key.trim())}
        submit={t("Add", "添加")}
        onCancel={onCancel}
      />
    </InlineForm>
  );
}

/** The type of a relay sniffing could not tell, chosen by hand for its
 * address: one of the common templates, or its own balance endpoint. */
export function RelayKindForm({
  t,
  pending,
  kind: chosen,
  custom: saved,
  onSave,
  onCancel,
}: {
  t: Text;
  pending: boolean;
  kind: RelayKind;
  custom?: RelayCustom;
  onSave: (choice: { kind: RelayKind; custom?: RelayCustom }) => void;
  onCancel: () => void;
}) {
  const [kind, setKind] = useState<RelayKind>(chosen);
  const [url, setUrl] = useState(saved?.url ?? "");
  const [remaining, setRemaining] = useState(saved?.remaining ?? "");
  const [divisor, setDivisor] = useState(String(saved?.divisor ?? 1));
  const [currency, setCurrency] = useState<"USD" | "CNY">(
    saved?.currency ?? "USD",
  );
  const [header, setHeader] = useState(saved?.header ?? "");
  const kinds: [RelayKind, string][] = [
    ["auto", t("Automatic", "自动识别")],
    ["newapi", "New API"],
    ["sub2api", "Sub2API"],
    ["openrouter", "OpenRouter"],
    ["custom", t("Custom endpoint", "自定义接口")],
  ];
  return (
    <InlineForm
      onSubmit={() =>
        onSave(
          kind === "custom"
            ? {
                kind,
                custom: {
                  url,
                  remaining,
                  divisor: Number(divisor) || 1,
                  currency,
                  header,
                },
              }
            : { kind },
        )
      }
    >
      <Row title={t("Type", "类型")}>
        <Menu
          label={t("Type", "类型")}
          value={kind}
          options={kinds}
          change={(value) => setKind(value as RelayKind)}
          width={160}
        />
      </Row>
      {kind === "custom" && (
        <>
          <FieldRow
            label={t("Balance endpoint", "余额接口")}
            hint={t("On the relay's own address.", "须在中转站自己的地址上。")}
            value={url}
            change={setUrl}
            placeholder="/api/balance"
          />
          <FieldRow
            label={t("Balance field", "余额字段")}
            value={remaining}
            change={setRemaining}
            placeholder="data.balance"
          />
          <FieldRow
            label={t("Divide by", "除数")}
            value={divisor}
            change={setDivisor}
            placeholder="1"
            decimal
            width={80}
          />
          <Row title={t("Currency", "币种")}>
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
          <FieldRow
            label={t("Key header", "Key 请求头")}
            hint={t("Optional", "可不填")}
            value={header}
            change={setHeader}
            placeholder="Authorization"
          />
        </>
      )}
      <FormActions
        t={t}
        pending={pending}
        ready={kind !== "custom" || Boolean(url.trim() && remaining.trim())}
        submit={t("Save", "保存")}
        onCancel={onCancel}
      />
    </InlineForm>
  );
}
