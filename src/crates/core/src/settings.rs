//! The app's settings and provider switches: defaults, validation, and the
//! settings file, which another copy of the app may have written since.
use super::*;

impl crate::AgentHudCore {
    /// The loaded preference, available before the first collection pass.
    pub fn launch_at_login_enabled(&self) -> bool {
        self.settings["launchAtLogin"] == true
    }

    pub fn client_hooks_enabled(&self) -> bool {
        self.settings["clientHooks"] == true
    }

    /// The clients whose opt-in approval hook the user turned on
    /// (`optInApprovals`); it goes in while client hooks are on.
    pub fn opt_in_approvals(&self) -> Vec<String> {
        self.settings["optInApprovals"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|id| opts_in(id))
            .map(str::to_owned)
            .collect()
    }

    pub fn antigravity_cli_settings(&self) -> Vec<PathBuf> {
        self.settings["antigravityCliSettings"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(PathBuf::from)
            .collect()
    }

    pub fn set_presentation(&mut self, key: &str, value: &Value) -> Result<Value, String> {
        let mut values = serde_json::Map::new();
        values.insert(key.into(), value.clone());
        self.set_presentation_values(&Value::Object(values))
    }

    pub fn set_presentation_values(&mut self, values: &Value) -> Result<Value, String> {
        let values = values
            .as_object()
            .ok_or("Presentation values must be an object")?;
        for (key, value) in values {
            if !matches!(
                key.as_str(),
                "appearance" | "animations" | "showMenuBarIcon" | "appearanceFollowsScreen"
            ) {
                return Err("This setting requires its dedicated host action".into());
            }
            Self::validate_setting(key, value)?;
        }
        self.reload_preferences()?;
        if values
            .iter()
            .all(|(key, value)| self.settings.get(key) == Some(value))
        {
            return Ok(self.settings.clone());
        }
        let previous = self.settings.clone();
        for (key, value) in values {
            self.settings[key] = value.clone();
        }
        if let Err(error) = self.save() {
            self.settings = previous;
            return Err(error);
        }
        Ok(self.settings.clone())
    }

    pub fn seed_initial_preferences(&mut self, values: &Value) -> Result<bool, String> {
        if fs::symlink_metadata(&self.settings_path).is_ok() {
            return Ok(false);
        }
        let values = values
            .get("settings")
            .unwrap_or(values)
            .as_object()
            .ok_or("Initial settings must be an object")?;
        let mut next = self.settings.clone();
        let mut copied = false;
        for (key, value) in values {
            if Self::validate_setting(key, value).is_ok() {
                next[key] = value.clone();
                copied = true;
            }
        }
        if !copied {
            return Ok(false);
        }
        let previous = std::mem::replace(&mut self.settings, next);
        if let Err(error) = self.save() {
            self.settings = previous;
            return Err(error);
        }
        self.requests
            .set_hold_ms(self.settings["approvalWaitMinutes"].as_u64().unwrap_or(10) * 60_000);
        Ok(true)
    }

    pub(crate) fn apply_setting(&mut self, key: &str, value: &Value) -> Result<(), String> {
        Self::validate_setting(key, value)?;
        let previous = self.settings.clone();
        self.settings[key] = value.clone();
        if let Err(error) = self.save() {
            self.settings = previous;
            return Err(error);
        }
        if key == "approvalWaitMinutes" {
            self.requests.set_hold_ms(value.as_u64().unwrap() * 60_000);
        }
        self.apply_user_prices();
        Ok(())
    }

    /// Prices calls by what the owner set on the price page; when it changed,
    /// every log's cost is worked out again, a few at a time, and the rates
    /// and statistics follow.
    pub(crate) fn apply_user_prices(&mut self) {
        if crate::pricing::set_user_prices(crate::pricing::user_prices_from(&self.settings)) {
            for head in self.logs.values_mut() {
                head.list_cost_priced = false;
            }
            self.meters.view = None;
            self.statistics.revision = self.statistics.revision.wrapping_add(1);
        }
    }

    fn validate_setting(key: &str, value: &Value) -> Result<(), String> {
        match key {
            "appearance" if ["dark", "light", "system"].contains(&value.as_str().unwrap_or("")) => {
            }
            "quotaDisplay" if matches!(value.as_str(), Some("consumed" | "remaining")) => {}
            "codexHomes" | "claudeHomes" | "antigravityCliSettings"
                if value.as_array().is_some_and(|homes| {
                    homes.iter().all(|home| {
                        home.as_str().is_some_and(|home| {
                            !home.is_empty() && home.trim() == home && !home.contains('\0')
                        })
                    })
                }) => {}
            "language" if value.as_str().is_some_and(|s| s.len() <= 32) => {}
            "animations"
            | "showMenuBarIcon"
            | "appearanceFollowsScreen"
            | "hudTools"
            | "claudeAnswers"
            | "agentMessages"
            | "agentMessagesAsUser"
            | "claudeMessages"
            | "deepseekMessages"
            | "readCopilotQuota"
            | "clientHooks"
            | "launchAtLogin"
            | "hudVisible"
                if value.is_boolean() => {}
            // Passes in a row agents may message each other before the user
            // looks; null for no limit.
            "agentMessagesLimit"
                if value.is_null() || value.as_u64().is_some_and(|v| (1..=1000).contains(&v)) => {}
            "approvalWaitMinutes"
                if value
                    .as_u64()
                    .is_some_and(|v| [1, 3, 5, 10, 30, 60].contains(&v)) => {}
            // Clients whose approval hook waits for the user to turn it on
            // (agents `Permission::opt_in`), each once.
            "optInApprovals"
                if value.as_array().is_some_and(|ids| {
                    ids.iter().enumerate().all(|(index, id)| {
                        id.as_str().is_some_and(opts_in) && !ids[..index].contains(id)
                    })
                }) => {}
            // The price page: the owner's prices per million tokens by model,
            // multipliers by relay host, and what a local host is.
            "customPrices"
                if value.as_object().is_some_and(|prices| {
                    prices.len() <= 500
                        && prices.iter().all(|(model, price)| {
                            let rate = |key: &str| {
                                price[key].is_null()
                                    || price[key].as_f64().is_some_and(|v| v >= 0.0)
                            };
                            !model.is_empty()
                                && model.len() <= 300
                                && matches!(price["currency"].as_str(), Some("USD" | "CNY"))
                                && ["input", "output", "cacheRead", "cacheWrite"]
                                    .into_iter()
                                    .all(rate)
                        })
                }) => {}
            "relayMultipliers"
                if value.as_object().is_some_and(|hosts| {
                    hosts.len() <= 64
                        && hosts.iter().all(|(host, times)| {
                            !host.is_empty()
                                && host.len() <= 260
                                && times.as_f64().is_some_and(|v| v > 0.0 && v <= 100.0)
                        })
                }) => {}
            "localKinds"
                if value.as_object().is_some_and(|hosts| {
                    hosts.len() <= 64
                        && hosts.iter().all(|(host, kind)| {
                            !host.is_empty()
                                && host.len() <= 260
                                && matches!(kind.as_str(), Some("model" | "gateway"))
                        })
                }) => {}
            // The relay addresses whose balance may be read with the key the
            // client already sends them (agents `relays::CONSENT`).
            "relayReading"
                if value.as_array().is_some_and(|hosts| {
                    hosts.len() <= 64
                        && hosts.iter().all(|host| {
                            host.as_str().is_some_and(|host| {
                                !host.is_empty()
                                    && host.len() <= 260
                                    && host.bytes().all(|b| b.is_ascii_graphic() && b != b'/')
                            })
                        })
                }) => {}
            _ => return Err("Unsupported setting or invalid value".into()),
        }
        Ok(())
    }

    pub(crate) fn reload_preferences(&mut self) -> Result<(), String> {
        if !self.settings_path.exists() {
            return Ok(());
        }
        let stored: Value =
            serde_json::from_slice(&fs::read(&self.settings_path).map_err(|e| e.to_string())?)
                .map_err(|e| format!("Preferences could not be read: {e}"))?;
        if let Some(values) = stored["settings"].as_object() {
            for (key, value) in values {
                if self.settings.get(key).is_some() {
                    self.settings[key] = value.clone();
                }
            }
        }
        self.requests.set_hold_ms(
            self.settings["approvalWaitMinutes"]
                .as_u64()
                .filter(|v| [1, 3, 5, 10, 30, 60].contains(v))
                .unwrap_or(10)
                * 60_000,
        );
        self.apply_user_prices();
        let enabled = serde_json::from_value(stored["providers"].clone())
            .map_err(|e| format!("Provider preferences could not be read: {e}"))?;
        if self.enabled != enabled {
            self.enabled = enabled;
            self.usage_projection = None;
            self.meters.view = None;
        }
        Ok(())
    }

    pub(crate) fn save(&self) -> Result<(), String> {
        let parent = self
            .settings_path
            .parent()
            .ok_or("Invalid settings location")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let bytes =
            serde_json::to_vec_pretty(&json!({"settings":self.settings,"providers":self.enabled}))
                .map_err(|e| e.to_string())?;
        let temporary = self.settings_path.with_extension("json.tmp");
        fs::write(&temporary, bytes)
            .and_then(|_| fs::rename(temporary, &self.settings_path))
            .map_err(|e| e.to_string())
    }
}

/// Whether the client with this id has a hook that goes in only once the
/// user turns it on.
fn opts_in(id: &str) -> bool {
    agent_hud_agents::hooks::hooks_by_id(id).is_some_and(agent_hud_agents::hooks::Hooks::opt_in)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quota_display_defaults_to_remaining_and_persists_only_supported_choices() {
        let home = tempfile::tempdir().unwrap();
        let mut core = AgentHudCore::new(vec![], home.path().to_owned());
        assert_eq!(core.settings["quotaDisplay"], "remaining");
        core.apply_setting("quotaDisplay", &json!("remaining"))
            .unwrap();
        let reopened = AgentHudCore::new(vec![], home.path().to_owned());
        assert_eq!(reopened.settings["quotaDisplay"], "remaining");
        for invalid in [json!("used"), json!(""), json!(true), json!(null)] {
            assert!(core.apply_setting("quotaDisplay", &invalid).is_err());
            assert_eq!(core.settings["quotaDisplay"], "remaining");
        }
        core.apply_setting("quotaDisplay", &json!("consumed"))
            .unwrap();
        assert_eq!(core.settings["quotaDisplay"], "consumed");
    }

    #[test]
    fn account_source_paths_persist_and_reject_invalid_entries() {
        for key in ["codexHomes", "claudeHomes", "antigravityCliSettings"] {
            let home = tempfile::tempdir().unwrap();
            let mut core = AgentHudCore::new(vec![], home.path().to_owned());
            assert_eq!(core.settings[key], json!([]));
            let homes = json!(["/work/codex-personal", "~/.codex-work"]);
            core.apply_setting(key, &homes).unwrap();
            let reopened = AgentHudCore::new(vec![], home.path().to_owned());
            assert_eq!(reopened.settings[key], homes);
            for invalid in [
                json!(null),
                json!("/work/codex"),
                json!([null]),
                json!([""]),
                json!([" "]),
                json!([" /work/codex"]),
                json!(["/work/codex "]),
                json!(["/work/\0codex"]),
            ] {
                assert!(core.apply_setting(key, &invalid).is_err());
                assert_eq!(core.settings[key], homes);
            }
            core.apply_setting(key, &json!([])).unwrap();
            assert_eq!(core.settings[key], json!([]));
        }
    }

    #[test]
    fn opt_in_approvals_start_empty_and_take_only_clients_whose_hook_waits_for_the_user() {
        let home = tempfile::tempdir().unwrap();
        let mut core = AgentHudCore::new(vec![], home.path().to_owned());
        assert_eq!(core.settings["optInApprovals"], json!([]));
        assert!(core.opt_in_approvals().is_empty());
        core.apply_setting("optInApprovals", &json!(["copilot"]))
            .unwrap();
        let reopened = AgentHudCore::new(vec![], home.path().to_owned());
        assert_eq!(reopened.opt_in_approvals(), ["copilot"]);
        for invalid in [
            json!(null),
            json!(true),
            json!("copilot"),
            json!([1]),
            json!(["copilot", "copilot"]),
            // Claude Code's approval hook goes in with client hooks already.
            json!(["claude"]),
            json!(["not-a-client"]),
            json!(["Copilot"]),
        ] {
            assert!(
                core.apply_setting("optInApprovals", &invalid).is_err(),
                "{invalid}"
            );
            assert_eq!(core.opt_in_approvals(), ["copilot"]);
        }
        core.apply_setting("optInApprovals", &json!([])).unwrap();
        assert!(core.opt_in_approvals().is_empty());
    }

    #[test]
    fn opt_in_approvals_read_from_the_file_keep_only_clients_whose_hook_waits_for_the_user() {
        let home = tempfile::tempdir().unwrap();
        fs::write(
            home.path().join("settings.json"),
            br#"{"settings":{"optInApprovals":["claude",7,"copilot"]},"providers":{}}"#,
        )
        .unwrap();
        let core = AgentHudCore::new(vec![], home.path().to_owned());
        assert_eq!(core.opt_in_approvals(), ["copilot"]);
    }

    #[test]
    fn old_preferences_without_quota_display_use_the_remaining_default() {
        let home = tempfile::tempdir().unwrap();
        fs::write(
            home.path().join("settings.json"),
            br#"{"settings":{"language":"en"},"providers":{}}"#,
        )
        .unwrap();
        let core = AgentHudCore::new(vec![], home.path().to_owned());
        assert_eq!(core.settings["language"], "en");
        assert_eq!(core.settings["quotaDisplay"], "remaining");
    }
}
