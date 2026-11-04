//! The HUD's decisions on Codex Desktop's cards, each sent once to the
//! thread's owner, and how a failed or unconfirmed send is reported.
use super::super::desktop_io::{endpoint, DesktopIpc, IpcError};
use super::pending::{self, Pending};
use super::Shared;
use agent_hud_agents::codex::AGENT;
use agent_hud_core::l10n;
use serde_json::Value;
use std::{
    sync::{atomic::Ordering, Arc, PoisonError},
    thread,
    time::Duration,
};

impl Shared {
    pub(super) fn answers(self: &Arc<Self>) {
        let mut ready = Vec::new();
        {
            let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            for live in state.pending.values_mut() {
                let Some(ticket) = live.ticket.as_ref() else {
                    continue;
                };
                match ticket.response.try_recv() {
                    Ok(Some(decision)) => {
                        ready.push((live.item.clone(), decision));
                        live.ticket = None;
                    }
                    Ok(None) | Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        live.ticket = None
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {}
                }
            }
        }
        for (item, decision) in ready {
            if !self.active.load(Ordering::Acquire) {
                return;
            }
            let shared = self.clone();
            let _ = thread::Builder::new()
                .name("Codex Desktop answer".into())
                .spawn(move || {
                    let result = shared.resolve(&item, &decision);
                    shared
                        .state
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .answer_connections
                        .remove(&item.id);
                    if shared.active.load(Ordering::Acquire) {
                        match result {
                            Ok(()) => shared.requests.note_native_delivery(
                                &item.id,
                                AGENT.id,
                                &item.thread,
                                "confirmed",
                                None,
                                crate::hooks::now_ms(),
                            ),
                            Err(ReplyFailure::Failed(reason)) => {
                                shared.requests.note_native_delivery(
                                    &item.id,
                                    AGENT.id,
                                    &item.thread,
                                    "failed",
                                    Some(reason),
                                    crate::hooks::now_ms(),
                                )
                            }
                            Err(ReplyFailure::Uncertain(reason)) => {
                                shared.requests.note_native_delivery(
                                    &item.id,
                                    AGENT.id,
                                    &item.thread,
                                    "uncertain",
                                    Some(reason),
                                    crate::hooks::now_ms(),
                                )
                            }
                            Err(ReplyFailure::Cancelled) => {}
                        }
                    }
                });
        }
    }
    fn resolve(&self, item: &Pending, decision: &Value) -> Result<(), ReplyFailure> {
        let (response, method, state_value) = {
            let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            let known = state.known.get(&item.thread).ok_or(IpcError::Unavailable)?;
            if known.owner != item.owner {
                return Err(IpcError::Unavailable.into());
            }
            let current = pending::parse(&known.state, &item.thread, &item.owner)
                .map_err(|_| IpcError::Protocol)?;
            if !current
                .iter()
                .any(|value| value.id == item.id && value.wire == item.wire)
            {
                return Err(IpcError::Unavailable.into());
            }
            let (method, response) = pending::answer(item, decision, &known.state)?;
            (response, method, known.state["cwd"].clone())
        };
        if response.is_null() {
            return Err(ReplyFailure::Cancelled);
        }
        let mut connection = DesktopIpc::connect(&endpoint(&self.data), Duration::from_secs(3))?;
        {
            let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            if !self.active.load(Ordering::Acquire) {
                return Err(ReplyFailure::Cancelled);
            }
            state
                .answer_connections
                .insert(item.id.clone(), connection.shutdown_handle()?);
        }
        let initialized = connection.request(
            "initialize",
            0,
            "initializing-client",
            None,
            serde_json::json!({"clientType":"agent-hud"}),
            None,
        )?;
        let client = initialized["result"]["clientId"]
            .as_str()
            .ok_or(IpcError::Protocol)?
            .to_string();
        let discovery = connection.request(
            "thread-owner-discovery",
            1,
            &client,
            None,
            serde_json::json!({"hostId":"local","conversationId":item.thread}),
            None,
        )?;
        if !self.active.load(Ordering::Acquire) {
            return Err(ReplyFailure::Cancelled);
        }
        if discovery["handledByClientId"] != item.owner {
            return Err(IpcError::Unavailable.into());
        }
        let params = if method == pending::ELICITATION_METHOD {
            serde_json::json!({"conversationId":item.thread,"requestId":item.request_id,"response":response})
        } else {
            let text = response.as_str().ok_or(IpcError::Protocol)?;
            let id = super::super::desktop_io::request_id()?;
            let mut context = serde_json::json!({"prompt":text,"turnTrigger":"send_user_message_async_question","addedFiles":[],"fileAttachments":[],"ideContext":null,"imageAttachments":[]});
            let mut restore = serde_json::json!({"id":id,"text":text,"context":context,"createdAt":crate::hooks::now_ms()});
            if let Some(cwd) = state_value.as_str() {
                context["workspaceRoots"] = serde_json::json!([cwd]);
                restore["context"] = context;
                restore["cwd"] = serde_json::json!(cwd);
            }
            serde_json::json!({"conversationId":item.thread,"clientUserMessageId":id,"input":[{"type":"text","text":text,"text_elements":[]}],"attachments":[],"restoreMessage":restore})
        };
        // A reply attempt is never automatically retried. Requests retains its
        // dismissed identity until the owner actually settles this exact request.
        let mut wrote = false;
        let receipt = connection
            .request(
                method,
                1,
                &client,
                Some(&item.owner),
                params,
                Some(&mut || wrote = true),
            )
            .map_err(|error| {
                if !self.active.load(Ordering::Acquire) {
                    ReplyFailure::Cancelled
                } else {
                    match &error {
                        IpcError::Rejected(reason)
                            if !matches!(
                                reason.as_str(),
                                "request-timeout" | "server-closed" | "client-disconnected"
                            ) =>
                        {
                            ReplyFailure::Failed(error.to_string())
                        }
                        _ if wrote => ReplyFailure::Uncertain(error.to_string()),
                        _ => ReplyFailure::Failed(error.to_string()),
                    }
                }
            })?;
        if receipt["handledByClientId"] != item.owner || receipt["result"]["method"] != method {
            return Err(ReplyFailure::Uncertain(l10n::text(
                "Codex Desktop 的回复与请求不匹配",
                "Desktop reply identity did not match",
            )));
        }
        if method == pending::ELICITATION_METHOD {
            if receipt["result"]["result"]["ok"] != true {
                return Err(ReplyFailure::Uncertain(l10n::text(
                    "Codex Desktop 没有确认这个审批",
                    "Desktop did not confirm this approval",
                )));
            }
        } else if receipt["result"]["result"]["result"]["turnId"]
            != item.turn_id.as_deref().unwrap_or("")
        {
            return Err(ReplyFailure::Uncertain(l10n::text(
                "Codex Desktop 没有确认对应的提问轮次",
                "Desktop did not confirm the exact question turn",
            )));
        }
        Ok(())
    }
}
enum ReplyFailure {
    Failed(String),
    Uncertain(String),
    Cancelled,
}
impl From<IpcError> for ReplyFailure {
    fn from(error: IpcError) -> Self {
        Self::Failed(error.to_string())
    }
}
