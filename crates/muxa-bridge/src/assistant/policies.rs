//! The user's policies (ADR 0048 §1): `add-policy`, `update-policy`, their
//! expiry, and their place in `inbox-list`. Only the user writes one; no run
//! does. Texts never reach a log or an error message.
use muxa_protocol::{ErrorBody, ErrorCode, Policy, PolicyAction};

use super::store::Shelf;
use super::work::{Fail, check_rev};
use crate::convert::error_body;
use crate::records::assistant::Role;
use crate::task_records::Config;

/// Policies that are not expired, at most (ADR 0048 §8).
pub const MAX_ACTIVE: usize = 50;
/// Bytes of a policy's text.
pub const MAX_TEXT: usize = 500;
/// Seconds an expired policy stays listed: 30 days.
pub const EXPIRED_KEEP: u64 = 30 * 86_400;
/// The scope of a policy for every workspace.
pub const ALL: &str = "all";
const SOURCE_USER: &str = "user";

/// The fields `add-policy` and `update-policy edit` take.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Fields {
    pub text: Option<String>,
    pub scope: Option<String>,
    pub from: Option<u64>,
    pub until: Option<u64>,
}

impl Fields {
    fn is_empty(&self) -> bool {
        self.text.is_none() && self.scope.is_none() && self.from.is_none() && self.until.is_none()
    }
}

pub struct Change {
    pub policy: String,
    pub rev: u64,
    pub action: PolicyAction,
    pub fields: Fields,
}

/// Past its `until`: listed under Expired, in no brief.
pub fn expired(p: &Policy, now: u64) -> bool {
    p.until.is_some_and(|until| until < now)
}

/// Expired for longer than [`EXPIRED_KEEP`]: removed.
fn gone(p: &Policy, now: u64) -> bool {
    p.until
        .is_some_and(|until| now.saturating_sub(until) > EXPIRED_KEEP)
}

/// Blocking. Stores a new policy from the user's words.
pub fn add(config: &Config, fields: Fields, now: u64) -> Result<Policy, ErrorBody> {
    let (Some(text), Some(scope)) = (&fields.text, &fields.scope) else {
        return Err(bad("add-policy needs a text and a scope"));
    };
    check_text(text)?;
    check_scope(config, scope)?;
    check_until(fields.until, now)?;
    check_order(fields.from, fields.until)?;
    let shelf = super::shelf::<Policy>(config)?;
    let _folder = shelf.lock_folder().map_err(|e| super::state_error(&e))?;
    check_room(&shelf, None, now)?;
    let policy = shelf
        .create(|id| {
            Ok::<_, Fail>(Policy {
                id,
                rev: 1,
                text: text.clone(),
                scope: scope.clone(),
                from: fields.from,
                until: fields.until,
                source: SOURCE_USER.into(),
                created_at: now,
                updated_at: now,
            })
        })
        .map_err(ErrorBody::from)?;
    tracing::info!(policy = %policy.id, scope = %policy.scope, "policy added");
    Ok(policy)
}

/// Blocking. The edited policy, or None after a delete.
pub fn update(config: &Config, c: Change, now: u64) -> Result<Option<Policy>, ErrorBody> {
    if !is_policy_id(&c.policy) {
        return Err(bad("update-policy: policy must be y- and 8 hex digits"));
    }
    let shelf = super::shelf::<Policy>(config)?;
    match c.action {
        PolicyAction::Edit => edit(config, &shelf, c, now).map(Some),
        PolicyAction::Delete => delete(&shelf, c).map(|()| None),
    }
}

fn edit(config: &Config, shelf: &Shelf<Policy>, c: Change, now: u64) -> Result<Policy, ErrorBody> {
    let f = &c.fields;
    if f.is_empty() {
        return Err(bad(
            "update-policy: edit needs a text, a scope, a from or an until",
        ));
    }
    if let Some(text) = &f.text {
        check_text(text)?;
    }
    if let Some(scope) = &f.scope {
        check_scope(config, scope)?;
    }
    check_until(f.until, now)?;
    // An edit can make an expired policy active again: the limit holds then too.
    let _folder = shelf.lock_folder().map_err(|e| super::state_error(&e))?;
    let edited = shelf
        .update(&c.policy, |p| -> Result<_, Fail> {
            check_rev("Policy", &p.id, p.rev, c.rev)?;
            let from = f.from.or(p.from);
            let until = f.until.or(p.until);
            check_order(from, until)?;
            let was_expired = expired(p, now);
            if let Some(text) = &f.text {
                p.text = text.clone();
            }
            if let Some(scope) = &f.scope {
                p.scope = scope.clone();
            }
            p.from = from;
            p.until = until;
            if was_expired && !expired(p, now) {
                check_room(shelf, Some(&p.id), now)?;
            }
            p.rev += 1;
            p.updated_at = now;
            Ok(p.clone())
        })?
        .ok_or_else(|| no_policy(&c.policy))?;
    tracing::info!(policy = %edited.id, scope = %edited.scope, rev = edited.rev, "policy edited");
    Ok(edited)
}

fn delete(shelf: &Shelf<Policy>, c: Change) -> Result<(), ErrorBody> {
    if !c.fields.is_empty() {
        return Err(bad(
            "update-policy: delete takes no text, scope, from or until",
        ));
    }
    let removed = shelf
        .remove_if(&c.policy, |p| -> Result<(), Fail> {
            Ok(check_rev("Policy", &p.id, p.rev, c.rev)?)
        })?
        .ok_or_else(|| no_policy(&c.policy))?;
    tracing::info!(policy = %removed.id, "policy deleted");
    Ok(())
}

/// Blocking. Active policies, oldest first, then expired ones, the most
/// recently expired first. Those expired for over 30 days are not listed.
pub fn list(config: &Config, now: u64) -> Result<Vec<Policy>, ErrorBody> {
    if !config.state_dir.as_ref().is_some_and(|d| d.is_dir()) {
        return Ok(Vec::new());
    }
    let all = super::shelf::<Policy>(config)?
        .all()
        .map_err(|e| super::state_error(&e))?;
    Ok(order(all, now))
}

fn order(all: Vec<Policy>, now: u64) -> Vec<Policy> {
    let (mut old, mut active): (Vec<_>, Vec<_>) = all
        .into_iter()
        .filter(|p| !gone(p, now))
        .partition(|p| expired(p, now));
    active.sort_by(|a, b| a.created_at.cmp(&b.created_at).then(a.id.cmp(&b.id)));
    old.sort_by(|a, b| b.until.cmp(&a.until).then(a.id.cmp(&b.id)));
    active.extend(old);
    active
}

/// Blocking. Removes the policies expired for over 30 days; the daemon's
/// tick calls it. A failure is logged and tried again next tick.
pub fn prune(config: &Config, now: u64) {
    let Some(dir) = config.state_dir.as_ref().filter(|d| d.is_dir()) else {
        return;
    };
    let shelf = match Shelf::<Policy>::open(dir) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "cannot open the policies");
            return;
        }
    };
    let Ok(all) = shelf.all() else { return };
    for p in all.iter().filter(|p| gone(p, now)) {
        // An edit since the listing may have renewed it.
        let removed = shelf.remove_if(&p.id, |stored| -> Result<(), Fail> {
            if gone(stored, now) {
                Ok(())
            } else {
                Err(bad("renewed").into())
            }
        });
        match removed {
            Ok(Some(_)) => tracing::info!(policy = %p.id, "expired policy removed"),
            Ok(None) | Err(Fail::Client(_)) => {}
            Err(Fail::State(e)) => {
                tracing::warn!(policy = %p.id, error = %e, "cannot remove an expired policy")
            }
        }
    }
}

/// Refused with the limit when [`MAX_ACTIVE`] policies other than `except` are active.
fn check_room(shelf: &Shelf<Policy>, except: Option<&str>, now: u64) -> Result<(), Fail> {
    let active = shelf
        .all()?
        .iter()
        .filter(|p| Some(p.id.as_str()) != except && !expired(p, now))
        .count();
    if active >= MAX_ACTIVE {
        return Err(error_body(
            ErrorCode::Conflict,
            format!("{MAX_ACTIVE} active policies: delete one first"),
        )
        .into());
    }
    Ok(())
}

/// 1 to [`MAX_TEXT`] bytes, one paragraph: no line break or other control
/// character. The error never quotes it.
fn check_text(text: &str) -> Result<(), ErrorBody> {
    let ok =
        !text.trim().is_empty() && text.len() <= MAX_TEXT && !text.chars().any(char::is_control);
    if ok {
        Ok(())
    } else {
        Err(bad(format!(
            "a policy's text must be 1 to {MAX_TEXT} bytes, one paragraph"
        )))
    }
}

/// `all`, or a workspace on this host other than the overall assistant's.
fn check_scope(config: &Config, scope: &str) -> Result<(), ErrorBody> {
    if scope == ALL {
        return Ok(());
    }
    match config.load_workspaces()?.get(scope) {
        Some(w) if w.role == Role::Overall => Err(bad(format!(
            "{scope} is the overall assistant's workspace: give all or another workspace"
        ))),
        Some(_) => Ok(()),
        None => Err(bad(format!("no workspace {scope}"))),
    }
}

/// A sent `until` is not past: the policy would start expired.
fn check_until(until: Option<u64>, now: u64) -> Result<(), ErrorBody> {
    match until {
        Some(until) if until < now => Err(bad("until is past")),
        _ => Ok(()),
    }
}

fn check_order(from: Option<u64>, until: Option<u64>) -> Result<(), ErrorBody> {
    match (from, until) {
        (Some(from), Some(until)) if until <= from => Err(bad("until must be after from")),
        _ => Ok(()),
    }
}

/// `y-` and 8 lowercase hex digits, as the store makes them.
fn is_policy_id(id: &str) -> bool {
    id.strip_prefix("y-").is_some_and(|h| {
        h.len() == 8
            && h.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

fn no_policy(id: &str) -> ErrorBody {
    bad(format!("no policy {id}"))
}

fn bad(message: impl Into<String>) -> ErrorBody {
    error_body(ErrorCode::BadRequest, message.into())
}

#[cfg(test)]
#[path = "policies_tests.rs"]
mod tests;
