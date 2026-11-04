//! The permission hook: how long the client waits, what can be answered
//! from here, the shape of its reply, and the user's own rules in the
//! client's settings that it reads before a call reaches the HUD.
use super::hooks_of;
use serde_json::Value;
use std::{collections::HashMap, path::Path};

/// The permission hook.
pub struct Permission {
    /// How long the client waits for a reply, in its own unit.
    pub timeout: u64,
    /// Tools whose question or plan cannot be answered from here; their
    /// requests stay with the client.
    pub unanswerable: &'static [&'static str],
    /// It applies a permanent rule it offered (`always`).
    pub rules: bool,
    /// It takes a plan's approval, or a request to keep planning.
    pub plans: bool,
    /// Its permission policy is no proof the user reviews the turn: the host
    /// confirms the turn's reviewer before holding a request.
    pub reviewer_gate: bool,
    /// Its transcript shows a request answered in the client's own window,
    /// so the host stops waiting for the HUD's answer once it does.
    pub settles_in_transcript: bool,
    /// A run nobody watches, whose requests stay with the client.
    pub unattended: Option<crate::process::UnattendedRule>,
    /// The user's own rules in the client's settings, which the hook reads
    /// before a call reaches the HUD (`screen`).
    pub policy: Option<PolicyRule>,
    /// Installed only once the user turns it on (the app's `optInApprovals`
    /// setting lists the client): the client asks this hook before its own
    /// rules and the session's approvals, so it holds calls the user would
    /// otherwise have allowed.
    pub opt_in: bool,
    pub reply: Reply,
}

/// The shape of a permission reply.
pub enum Reply {
    /// `{"hookSpecificOutput": {"hookEventName": "PermissionRequest", "decision": …}}`.
    HookSpecificOutput,
    /// `{"decision": "approve" | "block"}`.
    Decision,
    /// The decision itself, `{"behavior": "allow" | "deny", "message"?}`:
    /// Copilot CLI's `permissionRequest`, whose callback says `sessionId`,
    /// `toolName` and `toolInput`.
    Behavior,
    /// A `PreToolUse` callback, which runs before every tool call `matcher`
    /// names, answered with `{"hookSpecificOutput": {"hookEventName":
    /// "PreToolUse", "permissionDecision": "allow" | "deny"}}`. Only a call
    /// `asks` says the client would ask about, or one the hook marked
    /// (`RULE_ASKS`), is held; the others pass with no answer.
    PreToolUse {
        matcher: &'static str,
        asks: fn(&Value) -> bool,
    },
}

impl Reply {
    /// The event the permission hook is registered for.
    pub fn event(&self) -> &'static str {
        match self {
            Self::Behavior => "permissionRequest",
            Self::PreToolUse { .. } => "PreToolUse",
            Self::HookSpecificOutput | Self::Decision => "PermissionRequest",
        }
    }
}

impl Permission {
    /// The event the permission hook is registered for.
    pub fn event(&self) -> &'static str {
        self.reply.event()
    }
}

/// What the user's own rules in a client's settings say about a tool call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Policy {
    /// No rule of the user's names the call: the client's level decides, as
    /// the reply's `asks` says.
    Unruled,
    /// A rule lets the call run or blocks it, so the client never asks: the
    /// hook answers nothing and leaves it to the client.
    Settled,
    /// A rule makes the client ask whatever its level: the HUD holds it.
    Asks,
}

/// What a client's policy rule may read: the hook's own environment, the
/// user's home and files, each read by platform with a size limit.
pub struct Files<'a> {
    pub home: &'a Path,
    pub env: &'a HashMap<String, String>,
    /// A file's first `limit + 1` bytes, so the reader can tell a file over
    /// its limit from one that fits; None when it is missing, not a plain
    /// file or unreadable.
    pub read: &'a dyn Fn(&Path, u64) -> Option<Vec<u8>>,
}

impl Files<'_> {
    /// A file of at most `limit` bytes; None when it is missing, larger or
    /// unreadable.
    pub fn bounded(&self, path: &Path, limit: u64) -> Option<Vec<u8>> {
        (self.read)(path, limit).filter(|bytes| bytes.len() as u64 <= limit)
    }
}

/// A client's policy rule, given a permission callback's payload.
pub type PolicyRule = fn(&Value, &Files) -> Policy;

/// The key the hook adds to a callback whose call a rule of the user's makes
/// the client ask about, so the HUD holds it whatever `asks` says. One the
/// client sent itself is dropped first.
pub const RULE_ASKS: &str = "agentHudRuleAsks";

/// Applies the client's policy to a permission callback before it goes to
/// the HUD: false when a rule settles the call and the hook answers nothing.
/// Without `files` (no home known) the callback goes on as it came.
pub fn screen(source: &str, payload: &mut Value, files: Option<&Files>) -> bool {
    let Some(object) = payload.as_object_mut() else {
        return true;
    };
    object.remove(RULE_ASKS);
    let rule = hooks_of(source)
        .ok()
        .and_then(|(_, hooks)| hooks.permission.as_ref()?.policy);
    let (Some(rule), Some(files)) = (rule, files) else {
        return true;
    };
    match rule(payload, files) {
        Policy::Unruled => true,
        Policy::Settled => false,
        Policy::Asks => {
            if let Some(object) = payload.as_object_mut() {
                object.insert(RULE_ASKS.into(), Value::Bool(true));
            }
            true
        }
    }
}
