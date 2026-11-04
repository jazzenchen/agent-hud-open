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
