import {
  Page,
  Row,
  Section,
  SmallButton,
  useText,
  type SettingsPaneProps,
} from "./SettingsPrimitives";

/** Open builds have no update feed; a product build replaces this page. */
export function UpdatesPane({ snapshot }: SettingsPaneProps) {
  const t = useText(snapshot);
  return (
    <Page>
      <Section>
        <Row
          title={t("Automatic updates", "自动更新")}
          subtitle={t(
            "This build does not update itself.",
            "这个版本不会自动更新。",
          )}
        >
          <SmallButton disabled>{t("Unavailable", "不可用")}</SmallButton>
        </Row>
      </Section>
    </Page>
  );
}
