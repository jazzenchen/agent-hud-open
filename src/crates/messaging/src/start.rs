//! `start_session`: a new session whose first message is the caller's.
use super::*;

impl Messaging {
    pub(crate) fn start(
        &mut self,
        caller: &Caller,
        arguments: &Value,
        host: &Host<'_>,
        now: i64,
    ) -> Result<Value> {
        let zh = language(host.snapshot);
        let client = arguments["client"].as_str().ok_or("client is required")?;
        let text = arguments["text"].as_str().ok_or("text is required")?;
        if client == "deepseek" {
            return Ok(refused(&words::deepseek_no_start(zh)));
        }
        if !matches!(client, "claude" | "codex" | "antigravity") {
            return Ok(refused(&words::other_client(zh)));
        }
        if let Some(reason) = check_text(text, zh) {
            return Ok(refused(&reason));
        }
        let provider = provider(client);
        let offers = self.starts.offers_for(&all_sessions(host.snapshot));
        let Some(offer) = offers.iter().find(|offer| offer.provider == provider) else {
            return Ok(refused(&words::cannot_start(zh, client)));
        };
        let folder = match arguments["folder"].as_str() {
            Some(folder) => folder.to_owned(),
            None => find(host.snapshot, &caller.session_id)
                .and_then(|session| session["cwd"].as_str())
                .and_then(|cwd| self.starts.offered_folder(cwd))
                .unwrap_or_default(),
        };
        let offered = if folder.is_empty() {
            offer.without_folder
        } else {
            offer.folders.contains(&folder)
        };
        if !offered {
            return Ok(refused(&words::folder(zh, &folder, &offer.folders)));
        }
        let (chain, count) = self.next_pass(caller, host, None);
        let sender = party(host.snapshot, &caller.client, &caller.session_id);
        let newcomer = Party {
            client: client.into(),
            session_id: String::new(),
            title: None,
        };
        let from_left = chain
            .as_deref()
            .map(|chain| self.left_of(chain, &caller.session_id))
            .is_none_or(|left| same_session(&left, &caller.session_id));
        if let Some(limit) = self.preferences.limit.filter(|limit| count > *limit) {
            (self.fly)(flight(&sender, &newcomer, from_left, true, true));
            return Ok(refused(&words::limit(zh, limit)));
        }
        let id = message_id();
        let form = self.form();
        let from = party(host.snapshot, &caller.client, &caller.session_id);
        let delivered = words::compose(zh, form, &from, client, &id, None, text);
        let request_id = uuid::Uuid::new_v4().to_string();
        let receipt = self.starts.start_with(
            StartRequest {
                provider,
                request_id: request_id.clone(),
                text: delivered.clone(),
                folder,
                title: Some(text.to_owned()),
            },
            &offers,
        );
        if let Ok(receipt) = &receipt {
            self.clients.started(receipt, &caller.surface(), now);
            if receipt.handoff_pending {
                (self.follow)(receipt.clone());
            }
        }
        let (state, reason, session) = match &receipt {
            Ok(StartReceipt {
                state,
                reason,
                session_id,
                ..
            }) => (
                match state.as_str() {
                    "started" => "started",
                    "uncertain" => "uncertain",
                    _ => "refused",
                },
                reason.clone(),
                session_id.clone(),
            ),
            Err(error) => ("refused", Some(plain(&error.to_string())), None),
        };
        self.journal.add(Message {
            id: id.clone(),
            kind: Kind::Start,
            chain: chain.unwrap_or_else(|| id.clone()),
            count,
            from,
            to: Party {
                client: client.into(),
                session_id: session.clone().unwrap_or_default(),
                title: None,
            },
            form,
            reply_to: None,
            request_id,
            sent_at_ms: now,
            cursor: Some(0),
            fingerprint: transcript::fingerprint(&delivered),
            state: state.into(),
            reason: reason.clone(),
            turn: None,
            answered: false,
            replied_by: None,
        });
        let newcomer = Party {
            session_id: session.clone().unwrap_or_default(),
            ..newcomer
        };
        (self.fly)(flight(
            &sender,
            &newcomer,
            from_left,
            true,
            state == "refused",
        ));
        Ok(json!({"id":id,"state":state,"reason":reason,"sessionId":session,"pass":count}))
    }
}
