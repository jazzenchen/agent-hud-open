import { useT } from "@agent-hud/ui/format-context";
import {
  Page,
  Row,
  Section,
  type SettingsPaneProps,
} from "./SettingsPrimitives";
import { SmallButton } from "./SettingsControls";

/** Open builds have no update feed; a product build replaces this page. */
export function UpdatesPane(_props: SettingsPaneProps) {
  const t = useT();
  return (
    <Page>
      <Section>
        <Row
          title={t("Automatic updates")}
          subtitle={t("This build does not update itself.")}
        >
          <SmallButton disabled>{t("Unavailable")}</SmallButton>
        </Row>
      </Section>
    </Page>
  );
}
