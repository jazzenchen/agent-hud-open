use super::card::build_answer;
use super::observer::{is_harness, port_flag};
use super::*;

#[test]
fn deepseek_harness_discovery_frame_parsing_and_answer_receipt_match_source() {
    assert!(is_harness(
        "/opt/homebrew/bin/node /usr/local/lib/node_modules/deepseek-harness/bin.js --profile web --port 3199"
    ));
    assert_eq!(
        port_flag(
            "/opt/homebrew/bin/node /usr/local/lib/node_modules/deepseek-harness/bin.js --profile web --port 3199"
        ),
        Some(3199)
    );
    assert!(is_harness("dsh --profile=web"));
    assert_eq!(port_flag("dsh --profile=web"), None);
    assert!(!is_harness("dsh --profile cli"));

    let describe_reply = json!({
        "type": "server-response",
        "result": {
            "ok": true,
            "value": {
                "version": "0.4.2",
                "provider": "deepseek",
                "model": "deepseek-chat"
            }
        }
    });
    assert!(is_valid_describe(&describe_reply));

    let raw_frame = json!({
        "type": "server-request",
        "rpcId": "rpc_77",
        "method": "question/requested",
        "payload": {
            "sessionId": "sess_abc",
            "questions": [
                {
                    "id": "q_target",
                    "question": "Which target should we build?",
                    "header": "Target",
                    "multiSelect": false,
                    "options": [
                        {"label": "Debug", "description": "Fast compile"},
                        {"label": "Release", "description": "Optimized"}
                    ]
                }
            ]
        }
    });
    let frame = parse_frame(&raw_frame).expect("valid question frame");
    let pending = build_pending(frame, 3080);
    assert_eq!(pending.id, "deepseek:3080:rpc_77");
    assert_eq!(pending.session_id, "deepseek:sess_abc");
    assert_eq!(pending.view["answersQuestion"], true);

    let decision = json!({
        "kind": "answer",
        "answers": {
            "q_target": {
                "selected": ["Release"],
                "custom": "With LTO"
            }
        }
    });
    let answer_body = build_answer(&decision, &pending)
        .unwrap()
        .expect("answer payload");
    assert_eq!(answer_body["type"], "client-response");
    assert_eq!(answer_body["rpcId"], "rpc_77");
    assert_eq!(
        answer_body["result"]["value"]["sessionId"],
        json!("sess_abc")
    );
    assert_eq!(
        answer_body["result"]["value"]["answer"]["answers"],
        json!([{
            "id": "q_target",
            "selected": ["Release"],
            "custom": "With LTO"
        }])
    );

    let receipt_ok = json!({
        "type": "server-response",
        "result": {
            "ok": true,
            "value": { "accepted": true }
        }
    });
    assert_eq!(parse_receipt(&receipt_ok).unwrap(), Receipt::Accepted);

    let receipt_not_pending = json!({
        "type": "server-response",
        "result": {
            "ok": true,
            "value": { "accepted": false, "reason": "not-pending" }
        }
    });
    assert_eq!(
        parse_receipt(&receipt_not_pending).unwrap(),
        Receipt::NotPending
    );
}
