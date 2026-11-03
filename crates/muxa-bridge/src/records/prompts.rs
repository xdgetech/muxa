//! A prompt wait that outlives its process (ADR 0019): `pendingPrompt` says
//! which prompt the agent of a Task still waits for, since when, and for
//! which run. Kept in the record, so a daemon that starts resumes it.

use serde::{Deserialize, Serialize};

use super::Record;

const FIELD: &str = "pendingPrompt";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingPrompt {
    pub kind: PromptKind,
    /// When the wait began, in epoch seconds.
    pub since: u64,
    pub run: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PromptKind {
    Start,
    Restart,
}

impl Record {
    /// None also when the field is unreadable: a wait nobody can resume.
    pub fn pending_prompt(&self) -> Option<PendingPrompt> {
        // Even an unknown operation format suppresses automatic input.
        if self.extra.contains_key(super::lifecycle::FIELD) {
            return None;
        }
        let value = self.extra.get(FIELD)?;
        match serde_json::from_value(value.clone()) {
            Ok(p) => Some(p),
            Err(e) => {
                tracing::warn!(task = %self.id, error = %e, "unreadable pendingPrompt");
                None
            }
        }
    }

    pub fn set_pending_prompt(&mut self, prompt: Option<PendingPrompt>) {
        match prompt.map(serde_json::to_value) {
            Some(Ok(v)) => {
                self.extra.insert(FIELD.into(), v);
            }
            Some(Err(e)) => {
                tracing::warn!(task = %self.id, error = %e, "cannot encode pendingPrompt")
            }
            None => {
                self.extra.remove(FIELD);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wait_round_trips_through_the_record_file() {
        let mut r: Record = serde_json::from_value(serde_json::json!({
            "version": 1, "id": "t-0000aa", "rev": 1, "workspace": "w", "title": "t",
            "goal": "g", "status": "open", "createdAt": 1, "updatedAt": 1
        }))
        .unwrap();
        assert_eq!(r.pending_prompt(), None);
        let p = PendingPrompt {
            kind: PromptKind::Restart,
            since: 100,
            run: 2,
        };
        r.set_pending_prompt(Some(p));
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(
            json["pendingPrompt"],
            serde_json::json!({"kind": "restart", "since": 100, "run": 2})
        );
        let back: Record = serde_json::from_value(json).unwrap();
        assert_eq!(back.pending_prompt(), Some(p));
        r.set_pending_prompt(None);
        assert!(
            !serde_json::to_value(&r)
                .unwrap()
                .as_object()
                .unwrap()
                .contains_key("pendingPrompt")
        );
    }
}
