//! The Capture assistant's suggestion on an item (ADR 0084), and the
//! settings proposal of a Bot's talk (ADR 0083). Only the user's accept
//! changes the item.

use muxa_protocol::v2::{InboxItem, ItemKind, ItemSuggestion, SettingsProposal};

use super::{Book, Extra, InboxError, MAX_NOTE, MAX_TITLE, MAX_WORKSPACE, NewItem, checked};

/// Items one suggestion may split into.
const MAX_SPLIT: usize = 5;

impl Book {
    /// Puts the assistant's suggestion on the open item `id`, replacing an
    /// earlier one.
    pub fn suggest(&mut self, id: &str, suggestion: ItemSuggestion) -> Result<(), InboxError> {
        check(&suggestion)?;
        let at = self.open(id)?;
        self.items[at].item.suggestion = Some(suggestion);
        self.revision += 1;
        Ok(())
    }

    /// Accepts or dismisses the suggestion of `id`. Accept applies its kind,
    /// Workspace, due date and note; a split adds one item per title in this
    /// item's place, and this item becomes Done. Returns the item as it was
    /// and the suggestion, for the assistant to learn from.
    pub fn decide_suggestion(
        &mut self,
        id: &str,
        accept: bool,
        now: i64,
    ) -> Result<(InboxItem, ItemSuggestion), InboxError> {
        let at = self.open(id)?;
        let before = self.items[at].item.clone();
        let suggestion = before
            .suggestion
            .clone()
            .ok_or_else(|| InboxError::Invalid(format!("item {id} has no suggestion")))?;
        let item = &mut self.items[at].item;
        item.suggestion = None;
        self.revision += 1;
        if !accept {
            return Ok((before, suggestion));
        }
        if let Some(kind) = suggestion.kind {
            item.kind = kind;
        }
        if let Some(w) = &suggestion.workspace {
            item.workspace = Some(w.clone());
        }
        if let Some(d) = &suggestion.due {
            item.due = Some(d.clone());
        }
        if let Some(n) = &suggestion.note {
            let note = match item.note.take() {
                Some(old) if !old.trim().is_empty() => format!("{old}\n\n{n}"),
                _ => n.clone(),
            };
            item.note = Some(truncated(note, MAX_NOTE));
        }
        if suggestion.split.is_empty() {
            return Ok((before, suggestion));
        }
        let (kind, workspace, due) = (item.kind, item.workspace.clone(), item.due.clone());
        let source = item.source.clone();
        let parts: Vec<NewItem> = suggestion
            .split
            .iter()
            .map(|title| NewItem {
                kind,
                title: title.clone(),
                note: None,
                workspace: workspace.clone(),
                due: due.clone(),
                source: source.clone(),
                key: None,
                once: false,
                terminal: None,
                routine: None,
                place: None,
                extra: Extra::default(),
            })
            .map(checked)
            .collect::<Result<_, _>>()?;
        self.done(id)?;
        // Added last first, so the first part ends on top.
        for part in parts.into_iter().rev() {
            self.add(part, now)?;
        }
        Ok((before, suggestion))
    }

    /// A Bot of `workspace` renamed `from` → `to` (ADR 0089): its open and
    /// done items and its counts follow the new name, so its page keeps them.
    pub fn rename_bot(&mut self, workspace: &str, from: &str, to: &str) {
        let mut changed = false;
        for s in self.items.iter_mut().chain(self.done.iter_mut()) {
            let item = &mut s.item;
            if item.workspace.as_deref() == Some(workspace) && item.bot.as_deref() == Some(from) {
                item.bot = Some(to.to_owned());
                changed = true;
            }
        }
        if let Some(ledger) = self.bots.remove(&super::bots::key(workspace, from)) {
            self.bots.insert(super::bots::key(workspace, to), ledger);
            changed = true;
        }
        if changed {
            self.revision += 1;
        }
    }

    /// The settings proposal of the open item `id`.
    pub fn settings_proposal(&self, id: &str) -> Result<SettingsProposal, InboxError> {
        self.item(id)?
            .settings_proposal
            .clone()
            .ok_or_else(|| InboxError::Invalid(format!("item {id} proposes no settings")))
    }
}

fn check(s: &ItemSuggestion) -> Result<(), InboxError> {
    let bad = |what: &str| Err(InboxError::Invalid(what.to_owned()));
    if matches!(s.kind, Some(ItemKind::Review | ItemKind::Failed)) {
        return bad("a suggestion's kind is todo, read, decide or reply");
    }
    if s.workspace
        .as_ref()
        .is_some_and(|w| w.is_empty() || w.len() > MAX_WORKSPACE)
    {
        return bad("the suggested Workspace name is too long");
    }
    if let Some(due) = &s.due
        && due.parse::<jiff::civil::Date>().is_err()
    {
        return bad("the suggested due date is not YYYY-MM-DD");
    }
    if s.note.as_ref().is_some_and(|n| n.len() > MAX_NOTE)
        || s.task_goal.as_ref().is_some_and(|g| g.len() > MAX_NOTE)
    {
        return bad("a suggested note or goal has at most 4096 bytes");
    }
    if s.split.len() == 1 || s.split.len() > MAX_SPLIT {
        return bad("a split has 2 to 5 titles");
    }
    if s.split
        .iter()
        .any(|t| t.trim().is_empty() || t.chars().count() > MAX_TITLE)
    {
        return bad("a split title has 1 to 300 characters");
    }
    if s.bot
        .as_ref()
        .is_some_and(|b| b.is_empty() || b.len() > 64 || b.contains('/'))
    {
        return bad("a bot name is 1 to 64 bytes without /");
    }
    Ok(())
}

/// At most `max` bytes, cut at a character boundary.
fn truncated(mut text: String, max: usize) -> String {
    if text.len() > max {
        let mut end = max;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn captured(book: &mut Book, title: &str) -> String {
        let new = NewItem {
            kind: ItemKind::Todo,
            title: title.into(),
            note: None,
            workspace: None,
            due: None,
            source: "capture".into(),
            key: None,
            once: false,
            terminal: None,
            routine: None,
            place: None,
            extra: Extra::default(),
        };
        book.add(new, 1).unwrap().unwrap()
    }

    #[test]
    fn accept_applies_the_suggestion_and_dismiss_leaves_the_item() {
        let mut book = Book::default();
        let id = captured(&mut book, "call the bank");
        let s = ItemSuggestion {
            kind: Some(ItemKind::Decide),
            workspace: Some("home".into()),
            due: Some("2026-12-01".into()),
            note: Some("Ask about the fee.".into()),
            ..Default::default()
        };
        book.suggest(&id, s.clone()).unwrap();
        assert_eq!(book.item(&id).unwrap().suggestion.as_ref(), Some(&s));

        book.decide_suggestion(&id, true, 2).unwrap();
        let item = book.item(&id).unwrap();
        assert_eq!(
            (item.kind, item.workspace.as_deref(), item.due.as_deref()),
            (ItemKind::Decide, Some("home"), Some("2026-12-01"))
        );
        assert_eq!(item.note.as_deref(), Some("Ask about the fee."));
        assert!(item.suggestion.is_none());

        let other = captured(&mut book, "buy milk");
        book.suggest(&other, s).unwrap();
        book.decide_suggestion(&other, false, 3).unwrap();
        let item = book.item(&other).unwrap();
        assert_eq!(
            (item.kind, item.workspace.as_deref()),
            (ItemKind::Todo, None)
        );
    }

    #[test]
    fn a_split_replaces_the_item_with_its_parts_in_order() {
        let mut book = Book::default();
        let id = captured(&mut book, "plan the trip: book train, find hotel");
        let s = ItemSuggestion {
            workspace: Some("home".into()),
            split: vec!["Book the train".into(), "Find a hotel".into()],
            ..Default::default()
        };
        book.suggest(&id, s).unwrap();
        book.decide_suggestion(&id, true, 2).unwrap();
        let view = book.view();
        let titles: Vec<_> = view.items.iter().map(|i| i.title.as_str()).collect();
        assert_eq!(titles, ["Book the train", "Find a hotel"]);
        assert!(
            view.items
                .iter()
                .all(|i| i.workspace.as_deref() == Some("home"))
        );
        assert_eq!(view.done[0].id, id);
    }

    #[test]
    fn a_renamed_bots_items_and_counts_follow_it() {
        let mut book = Book::default();
        let mut new = NewItem {
            kind: ItemKind::Todo,
            title: "t".into(),
            note: None,
            workspace: Some("Lisk".into()),
            due: None,
            source: "routine".into(),
            key: Some("k".into()),
            once: false,
            terminal: None,
            routine: None,
            place: None,
            extra: Extra::default(),
        };
        new.extra.bot = Some("cheif".into());
        book.add_from_bot(new, "2026-11-24", 1).unwrap();
        book.rename_bot("Lisk", "cheif", "chief");
        let view = book.view();
        assert_eq!(view.items[0].bot.as_deref(), Some("chief"));
        assert_eq!(
            (view.bots[0].bot.as_str(), view.bots[0].added),
            ("chief", 1)
        );
    }

    #[test]
    fn a_bad_suggestion_is_refused() {
        let mut book = Book::default();
        let id = captured(&mut book, "x");
        let bad = [
            ItemSuggestion {
                kind: Some(ItemKind::Review),
                ..Default::default()
            },
            ItemSuggestion {
                due: Some("tomorrow".into()),
                ..Default::default()
            },
            ItemSuggestion {
                split: vec!["only one".into()],
                ..Default::default()
            },
        ];
        for s in bad {
            assert!(book.suggest(&id, s).is_err());
        }
        assert!(book.decide_suggestion(&id, true, 2).is_err());
    }
}
