import { invoke, isTauri } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { emit } from "@tauri-apps/api/event";
import { guideEvents } from "../guide/use-hud-guide";
import {
  Menu,
  Segments,
  SmallButton,
  ToggleRow,
  Unavailable,
} from "./SettingsControls";
import { languages } from "@agent-hud/ui/format";
import { useT } from "@agent-hud/ui/format-context";
import {
  Notice,
  Page,
  Row,
  Section,
  type SettingsPaneProps,
} from "./SettingsPrimitives";
import { usePending } from "./use-pending";

const appearanceKeys = ["appearance", "appearanceFollowsScreen", "language"];
const usageKeys = ["quotaDisplay"];
const startupKeys = ["launchAtLogin", "showMenuBarIcon"];

export function GeneralPane({ snapshot, action }: SettingsPaneProps) {
  useEffect(() => {
    if (isTauri())
      void invoke("native_system_status").catch((error) =>
        console.warn("System preferences could not be refreshed", error),
      );
  }, []);
  const t = useT();
  const { pending, error, run } = usePending();
  const [savedKey, setSavedKey] = useState("");

  /** One save at a time; a failure shows in the card of the setting that
   * failed. */
  const save = (key: string, value: unknown) => {
    if (pending) return;
    setSavedKey(key);
    void run(() => action("set_setting", { key, value }));
  };
  const setting = (key: string) => (value: unknown) => save(key, value);
  const problem = (keys: string[]) =>
    error && keys.includes(savedKey) ? (
      <Notice tone="error">{error}</Notice>
    ) : null;

  const settings = snapshot.settings;
  const followsScreen = Boolean(settings.appearanceFollowsScreen);
  const login = snapshot.connection.system?.launchAtLogin;
  const loginReason = login?.supported
    ? undefined
    : t("Launch at login isn't available in this build.");
  // Only states the person can act on; other notices are platform plumbing.
  const loginNote = !login?.supported
    ? null
    : login.status === "requires-approval"
      ? t("Allow it in System Settings → General → Login Items.")
      : login.status === "unavailable" && login.notice
        ? t("This build can't find the login service.")
        : null;

  const appearance = (
    <Segments
      label={t("Appearance")}
      value={settings.appearance}
      disabled={followsScreen}
      options={[
        ["system", t("System")],
        ["light", t("Light")],
        ["dark", t("Dark")],
      ]}
      change={setting("appearance")}
    />
  );

  return (
    <Page>
      <Section title={t("Appearance & language")}>
        <Row title={t("Appearance")}>
          {followsScreen ? (
            <Unavailable reason={t("Turn off Match screen to choose.")}>
              {appearance}
            </Unavailable>
          ) : (
            appearance
          )}
        </Row>
        <ToggleRow
          title={t("Match screen")}
          tag={t("Experimental")}
          subtitle={t("Windows take the HUD look of their screen.")}
          checked={settings.appearanceFollowsScreen ?? false}
          change={setting("appearanceFollowsScreen")}
        />
        <Row title={t("Language")}>
          <Menu
            label={t("Language")}
            value={
              // Settings before 0.7 stored Simplified Chinese as "zh-CN".
              settings.language === "zh-CN" ? "zh-Hans" : settings.language
            }
            options={[
              ["system", t("System")],
              ...languages.map(([code, name]): [string, string] => [
                code,
                name,
              ]),
            ]}
            change={setting("language")}
          />
        </Row>
        {problem(appearanceKeys)}
      </Section>
      <Section title={t("Usage display")}>
        <Row title={t("Quota percentage")}>
          <Menu
            label={t("Quota percentage")}
            value={settings.quotaDisplay ?? "consumed"}
            options={[
              ["consumed", t("Used")],
              ["remaining", t("Remaining")],
            ]}
            change={setting("quotaDisplay")}
          />
        </Row>
        {problem(usageKeys)}
      </Section>
      <Section title={t("Startup & menu bar")}>
        <ToggleRow
          title={t("Launch at login")}
          checked={login?.enabled ?? settings.launchAtLogin}
          disabled={pending || !login?.supported}
          reason={loginReason}
          change={setting("launchAtLogin")}
        />
        {loginNote && <Notice tone="warning">{loginNote}</Notice>}
        <ToggleRow
          title={t("Show menu bar icon")}
          checked={settings.showMenuBarIcon !== false}
          change={setting("showMenuBarIcon")}
        />
        {problem(startupKeys)}
      </Section>
      <Section title={t("Help")}>
        <Row
          title={t("Getting started")}
          subtitle={t("Walk through the HUD and its key settings again.")}
        >
          <SmallButton onClick={() => void emit(guideEvents.open, "tour")}>
            {t("Start again")}
          </SmallButton>
        </Row>
      </Section>
    </Page>
  );
}
