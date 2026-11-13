use super::credentials::{credential_key_from_secret, open_value};
use super::*;
use crate::account::fake::FakeHost;
use crate::account::HttpFailure;
use serde_json::json;

fn home() -> String {
    std::env::temp_dir()
        .join("agent-hud-zcode-fixture")
        .to_string_lossy()
        .into_owned()
}

#[test]
fn decrypts_official_enc_v1_fixture_and_queries_zcode_balance_with_bundle_version() {
    let key =
        credential_key_from_secret("zcode-credential-fallback:darwin:/Users/zcode-test:test-user");
    let cipher = "enc:v1:AAECAwQFBgcICQoL.NTIF8rgqI66J7hvPIwTD8g.QTtgwDlfAEvz72ttQggYC2KZyVwLVA";
    assert_eq!(
        open_value(cipher, &key).as_deref(),
        Some("official-fixture-token")
    );

    let base = home();
    let creds_path = std::path::Path::new(&base).join(".zcode/v2/credentials.json");
    let plist_path = std::path::Path::new(&base).join("Applications/ZCode.app/Contents/Info.plist");
    let host = FakeHost::new(&base)
        .env(
            "ZCODE_CREDENTIAL_SECRET",
            "zcode-credential-fallback:darwin:/Users/zcode-test:test-user",
        )
        .file(
            plist_path.to_str().unwrap(),
            "<plist><dict><key>CFBundleShortVersionString</key><string>3.17.2</string></dict></plist>",
        )
        .file(
            creds_path.to_str().unwrap(),
            &json!({
                "zcodejwttoken": cipher,
                "oauth:active_provider": "bigmodel",
                "oauth:bigmodel:user_info": json!({
                    "id": "z-user-1",
                    "email": "zcode@example.com",
                    "name": "ZCode User"
                }).to_string(),
            })
            .to_string(),
        )
        .answer_for(
            "https://zcode.z.ai/api/v1/zcode-plan/billing/balance?app_version=3.17.2",
            "Bearer official-fixture-token",
            Ok(json!({
                "success": true,
                "data": {
                    "plans": [{"name": "ZCode Pro", "status": "active"}],
                    "balances": [{
                        "name": "Monthly Credits",
                        "total_units": 1000,
                        "used_units": 250,
                        "remaining_units": 750,
                        "period_end": 1_796_083_200_i64
                    }]
                }
            })),
        );

    let reading = read(&host).unwrap();
    assert_eq!(reading.provider, "zcode");
    assert_eq!(reading.quotas.len(), 1);
    assert_eq!(reading.quotas[0]["vendor"], "zcode");
    assert_eq!(reading.quotas[0]["usedPercent"], 25.0);
    assert_eq!(reading.quotas[0]["nativeUsage"]["capacity"], 1000.0);
    assert_eq!(reading.quotas[0]["nativeUsage"]["unit"], "units");
    let acc = reading.account.unwrap();
    assert_eq!(acc["provider"], "zcode");
    assert_eq!(acc["plan"], "ZCode Pro");
    assert_eq!(acc["email"], "zcode@example.com");

    // 401 maps to signedOut.
    let expired = host.answer_for(
        "https://zcode.z.ai/api/v1/zcode-plan/billing/balance?app_version=3.17.2",
        "Bearer official-fixture-token",
        Err(HttpFailure::Status(401)),
    );
    assert_eq!(read(&expired).err().unwrap().kind(), "signedOut");
}
