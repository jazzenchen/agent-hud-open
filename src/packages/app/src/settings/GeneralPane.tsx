import { invoke, isTauri } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { Menu, Segments, ToggleRow, Unavailable } from "./SettingsControls";
import {
  Notice,
  Page,
  Row,
  Section,
  useText,
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
  const t = useText(snapshot);
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
    : t(
        login?.notice ?? "Not available in this build.",
        "此版本不支持登录时启动。",
      );
  // Only states the person can act on; other notices are platform plumbing.
  const loginNote = !login?.supported
    ? null
    : login.status === "requires-approval"
      ? t(
          login.notice ??
            "Allow it in System Settings → General → Login Items.",
          "需在「系统设置 → 通用 → 登录项」中允许。",
        )
      : login.status === "unavailable" && login.notice
        ? t(login.notice, "此版本找不到登录服务。")
        : null;

  const appearance = (
    <Segments
      label={t("Appearance", "外观")}
      value={settings.appearance}
      disabled={followsScreen}
      options={[
        ["system", t("System", "跟随系统")],
        ["light", t("Light", "浅色")],
        ["dark", t("Dark", "深色")],
      ]}
      change={setting("appearance")}
    />
  );

  return (
    <Page>
      <Section title={t("Appearance & language", "外观与语言")}>
        <Row title={t("Appearance", "外观")}>
          {followsScreen ? (
            <Unavailable
              reason={t(
                "Turn off Match screen to choose.",
                "关闭「跟随所在屏幕」后可选。",
              )}
            >
              {appearance}
            </Unavailable>
          ) : (
            appearance
          )}
        </Row>
        <ToggleRow
          title={t("Match screen", "跟随所在屏幕")}
          tag={t("Experimental", "实验")}
          subtitle={t(
            "Windows take the HUD look of their screen.",
            "窗口外观跟随所在屏幕的 HUD。",
          )}
          checked={settings.appearanceFollowsScreen ?? false}
          change={setting("appearanceFollowsScreen")}
        />
        <Row title={t("Language", "语言")}>
          <Menu
            label={t("Language", "语言")}
            value={
              settings.language === "zh-Hans" ? "zh-CN" : settings.language
            }
            options={[
              ["system", t("System", "跟随系统")],
              ["zh-CN", "中文"],
              ["en", "English"],
            ]}
            change={setting("language")}
          />
        </Row>
        {problem(appearanceKeys)}
      </Section>
      <Section title={t("Usage display", "用量显示")}>
        <Row
          title={t("Quota percentage", "额度百分比")}
          subtitle={t(
            "Remaining: a higher percentage means more available. Amount balances stay in their original currency.",
            "剩余：百分比越大，可用额度越多；金额余额保持原币种。",
          )}
        >
          <Menu
            label={t("Quota percentage", "额度百分比")}
            value={settings.quotaDisplay ?? "remaining"}
            options={[
              ["remaining", t("Remaining", "剩余")],
              ["consumed", t("Used", "已用")],
            ]}
            change={setting("quotaDisplay")}
          />
        </Row>
        {problem(usageKeys)}
      </Section>
      <Section title={t("Startup & menu bar", "启动与菜单栏")}>
        <ToggleRow
          title={t("Launch at login", "登录时启动")}
          checked={login?.enabled ?? settings.launchAtLogin}
          disabled={pending || !login?.supported}
          reason={loginReason}
          change={setting("launchAtLogin")}
        />
        {loginNote && <Notice tone="warning">{loginNote}</Notice>}
        <ToggleRow
          title={t("Show menu bar icon", "显示菜单栏图标")}
          checked={settings.showMenuBarIcon !== false}
          change={setting("showMenuBarIcon")}
        />
        {problem(startupKeys)}
      </Section>
    </Page>
  );
}
