//! The Bot actions of an Inbox item: Apply puts a talk's settings proposal
//! through the host's settings (ADR 0083), and a suggestion is accepted or
//! dismissed (ADR 0084). Only the user's request changes anything.

use muxa_inbox::{Inbox, InboxError};
use muxa_protocol::v2::ErrorCode;

use crate::session::task_code;
use crate::tasks::{Host, TaskError};

pub(crate) type Refusal = (ErrorCode, String);

/// `inbox-apply`: the proposal's ops with its revision, then the item is Done.
pub(crate) async fn apply<H: Host>(host: &H, inbox: &Inbox, id: &str) -> Result<(), Refusal> {
    let proposal = inbox
        .change(|b, _| b.settings_proposal(id))
        .map_err(refused)?;
    match host.settings_update(&proposal.revision, proposal.ops).await {
        Ok(_) => {}
        Err(TaskError::Stale(_)) => {
            return Err((
                ErrorCode::Conflict,
                "the settings changed since the Bot proposed this; ask it again".into(),
            ));
        }
        Err(e) => return Err((task_code(&e), e.to_string())),
    }
    inbox.change(|b, _| b.done(id)).map_err(refused)
}

/// `inbox-suggestion`: accept applies it, dismiss drops it.
pub(crate) async fn suggestion<H: Host>(
    host: &H,
    inbox: &Inbox,
    id: &str,
    accept: bool,
) -> Result<(), Refusal> {
    let (item, suggestion) = inbox
        .change(|b, now| b.decide_suggestion(id, accept, now))
        .map_err(refused)?;
    host.suggestion_decided(&item, &suggestion, accept).await;
    Ok(())
}

fn refused(e: InboxError) -> Refusal {
    let code = match e {
        InboxError::Invalid(_) => ErrorCode::BadRequest,
        InboxError::NotFound(_) => ErrorCode::NotFound,
    };
    (code, e.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use muxa_inbox::{Extra, NewItem};
    use muxa_protocol::v2::{ItemKind, ItemSuggestion, Settings, SettingsOp, SettingsProposal};
    use serde_json::json;

    use super::*;
    use crate::memory_settings::MemorySettings;
    use crate::tasks::{Prepared, TaskSpec};

    struct Host {
        settings: MemorySettings,
        decided: Mutex<Vec<bool>>,
    }

    impl crate::tasks::Host for Host {
        async fn prepare(&self, _spec: &TaskSpec) -> Result<Prepared, TaskError> {
            Err(TaskError::Invalid("no Tasks here".into()))
        }

        async fn settings_update(
            &self,
            revision: &str,
            ops: Vec<SettingsOp>,
        ) -> Result<Settings, TaskError> {
            self.settings.update(revision, ops)
        }

        async fn suggestion_decided(
            &self,
            _item: &muxa_protocol::v2::InboxItem,
            _suggestion: &ItemSuggestion,
            accepted: bool,
        ) {
            self.decided.lock().unwrap().push(accepted);
        }
    }

    fn host() -> Host {
        let start: Settings = serde_json::from_value(json!({
            "revision": "r0", "read_only": false, "problem": null, "workers": [],
            "workspaces": [{"name": "w", "root": "/w", "agent": {"command": null, "env": []}}]
        }))
        .unwrap();
        Host {
            settings: MemorySettings::new(start),
            decided: Mutex::new(Vec::new()),
        }
    }

    fn proposal_item(inbox: &Inbox, revision: &str) -> String {
        let ops = vec![
            serde_json::from_value(json!({"op": "bot-add", "workspace": "w",
                "bot": {"name": "b", "enabled": true}}))
            .unwrap(),
        ];
        let new = NewItem {
            kind: ItemKind::Decide,
            title: "b proposes: me".into(),
            note: None,
            workspace: Some("w".into()),
            due: None,
            source: "task".into(),
            key: None,
            once: false,
            terminal: None,
            routine: None,
            place: None,
            extra: Extra {
                settings_proposal: Some(SettingsProposal {
                    revision: revision.into(),
                    ops,
                }),
                ..Extra::default()
            },
        };
        inbox.change(|b, now| b.add(new, now)).unwrap().unwrap()
    }

    #[tokio::test]
    async fn apply_puts_the_proposal_through_settings_and_a_stale_one_changes_nothing() {
        let h = host();
        let inbox = Inbox::memory();
        let stale = proposal_item(&inbox, "old");
        let refused = apply(&h, &inbox, &stale).await;
        assert_eq!(refused.unwrap_err().0, ErrorCode::Conflict);
        assert!(inbox.item(&stale).is_ok(), "the item stays open");

        let revision = h.settings.view().revision;
        let fresh = proposal_item(&inbox, &revision);
        apply(&h, &inbox, &fresh).await.unwrap();
        assert!(inbox.item(&fresh).is_err(), "the item is Done");
        assert_eq!(h.settings.view().workspaces[0].bots[0].spec.name, "b");
    }

    #[tokio::test]
    async fn a_decided_suggestion_reaches_the_host() {
        let h = host();
        let inbox = Inbox::memory();
        let id = proposal_item(&inbox, "x");
        let s = ItemSuggestion {
            due: Some("2026-12-01".into()),
            ..ItemSuggestion::default()
        };
        inbox.change(|b, _| b.suggest(&id, s.clone())).unwrap();
        suggestion(&h, &inbox, &id, true).await.unwrap();
        assert_eq!(inbox.item(&id).unwrap().due.as_deref(), Some("2026-12-01"));
        assert_eq!(*h.decided.lock().unwrap(), [true]);
        assert!(
            suggestion(&h, &inbox, &id, false).await.is_err(),
            "no suggestion left"
        );
    }
}
