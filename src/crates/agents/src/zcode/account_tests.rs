use super::credentials::{credential_key_from_secret, open_value, Opener};
use super::*;
use crate::account::fake::FakeHost;
use crate::account::Answer;
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

const V2_PATH: &str = "/tmp/zc/.zcode/v2/credentials.json";
/// SHA-256 of `V2_PATH`, the account ZCode 4.0 files the key under.
const V2_ACCOUNT: &str = "7705846726fe089de3f93b1eb4ad0e7582a7e91a92516cb49f3564700e7f0807";
/// Sealed by Node's crypto as ZCode 4.0 does, under 32 bytes of 7.
const V2_OS: &str = "enc:v2:os:4bb06f8e4e3a7715d201d573:AQEBAQEBAQEBAQEB.olvYDp-KcMF_owVnlh9o6A.ANOk0fnHmHujsfFVH1wVXw";
const V2_OS_KEY: &str = "BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc";

fn v2_host(key: Answer<String>) -> FakeHost {
    let mut host = FakeHost::new("/tmp/zc");
    host.secrets
        .insert(format!("ZCode Credential Encryption|{V2_ACCOUNT}"), key);
    host
}

#[test]
fn opens_zcode_4_values_with_the_key_from_the_credential_store() {
    let host = v2_host(Answer::Found(format!("{V2_OS_KEY}\n")));
    let opener = Opener::new(&host, V2_PATH.into());
    assert_eq!(opener.open(V2_OS).unwrap(), "v2-fixture-token");
}

#[test]
fn zcode_4_values_under_another_key_are_not_unlocked() {
    let host = v2_host(Answer::Found(
        "CAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAg".into(),
    ));
    let opener = Opener::new(&host, V2_PATH.into());
    assert_eq!(
        opener.open(V2_OS).unwrap_err().reason(),
        "ZCode's encrypted sign-in could not be unlocked."
    );
}

#[test]
fn zcode_4_says_why_its_key_could_not_be_read_instead_of_signed_out() {
    for (answer, kind, reason) in [
        (Answer::Denied, "failed", "ZCode encrypted its sign-in, and keychain access was refused. Agent HUD asks again after it restarts."),
        (Answer::Missing, "failed", "ZCode encrypted its sign-in, but its keychain item is missing."),
        (Answer::Unsupported, "failed", "ZCode encrypted its sign-in; this system cannot unlock it yet."),
    ] {
        let host = v2_host(answer).file(
            V2_PATH,
            &json!({ "zcodejwttoken": V2_OS }).to_string(),
        );
        let unread = read(&host).err().unwrap();
        assert_eq!((unread.kind(), unread.reason()), (kind, reason));
        assert!(host.asked.borrow().is_empty());
    }
}

#[test]
fn opens_zcode_4_values_under_a_managed_secret_file() {
    let host = FakeHost::new("/tmp/zc").file(
        "/tmp/zc/.zcode/v2/credential-secret",
        "managed-secret-0123456789abcdefghijklmnop",
    );
    let opener = Opener::new(&host, V2_PATH.into());
    assert_eq!(
        opener
            .open("enc:v2:env:0653c83e72b32bebce1be480:AQEBAQEBAQEBAQEB.eYr2PijK4Y9BtSsY-yibbA.QtRkl4D0T-Q99NXI")
            .unwrap(),
        "v2-env-token"
    );
}

#[test]
fn an_unknown_zcode_format_is_not_sent_as_a_token() {
    let host = FakeHost::new("/tmp/zc").file(
        V2_PATH,
        &json!({ "zcodejwttoken": "enc:v3:something" }).to_string(),
    );
    assert_eq!(
        read(&host).err().unwrap().reason(),
        "ZCode encrypted its sign-in; this system cannot unlock it yet."
    );
    assert!(host.asked.borrow().is_empty());
}
