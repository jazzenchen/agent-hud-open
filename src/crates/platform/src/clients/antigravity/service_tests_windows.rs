/// Opt in only with an already running, signed-in client. Do not print the
/// returned account or the credential carried by its process command line.
#[test]
#[ignore = "requires a signed-in Antigravity desktop on this machine"]
fn reads_an_existing_windows_desktop_account() {
    let params = serde_json::json!({"metadata":{"ideName":"antigravity","extensionName":"antigravity","locale":"en","ideVersion":"unknown"}});
    let response = super::read_accounts(&params).expect("read-only account discovery failed");
    assert!(
        response["accounts"]
            .as_array()
            .is_some_and(|entries| !entries.is_empty()),
        "no authenticated account responded"
    );
    assert!(
        response["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|account| {
                let configs = account["status"]["userStatus"]["cascadeModelConfigData"]
                    ["clientModelConfigs"]
                    .as_array();
                let groups = account["summary"]["response"]["groups"]
                    .as_array()
                    .or_else(|| account["summary"]["groups"].as_array());
                configs.is_some_and(|configs| {
                    configs
                        .iter()
                        .any(|config| config["quotaInfo"]["remainingFraction"].is_number())
                }) || groups.is_some_and(|groups| {
                    groups.iter().any(|group| {
                        group["buckets"].as_array().is_some_and(|buckets| {
                            buckets.iter().any(|bucket| {
                                bucket["remainingFraction"].is_number()
                                    || bucket["remaining_fraction"].is_number()
                            })
                        })
                    })
                })
            }),
        "the account response has no numeric quota remaining fraction"
    );
}

#[test]
#[ignore = "requires a signed-in Antigravity desktop on this machine"]
fn local_service_call_endpoints_and_model_list_work_on_windows() {
    let params = serde_json::json!({"metadata":{"ideName":"antigravity","extensionName":"antigravity","locale":"en","ideVersion":"unknown"}});
    let status = super::call("GetUserStatus", &params).expect("service::call GetUserStatus failed");
    assert!(status["userStatus"].is_object());
    let endpoints = super::endpoints(true).expect("service::endpoints failed");
    assert!(!endpoints.is_empty(), "expected at least one endpoint");
    let trajectories = endpoints[0]
        .call("GetAllCascadeTrajectories", &serde_json::json!({}))
        .expect("GetAllCascadeTrajectories failed");
    assert!(trajectories.is_object());
    let temp = tempfile::tempdir().unwrap();
    let starts = crate::delivery::SessionStarts::new(
        crate::delivery::StartConfig {
            home: temp.path().to_path_buf(),
            claude_config: temp.path().join("claude"),
            codex_data: temp.path().join("codex"),
            claude_executable: None,
            codex_executable: None,
            antigravity_executable: None,
            claude_desktop: false,
            codex_desktop: false,
            task_root: temp.path().join("tasks"),
            environment: Default::default(),
        },
        temp.path().join("receipts"),
    )
    .unwrap();
    let models = starts
        .models(crate::delivery::Provider::Antigravity)
        .expect("SessionStarts::models(Antigravity) failed");
    assert!(!models.models.is_empty(), "expected Antigravity models");
}
