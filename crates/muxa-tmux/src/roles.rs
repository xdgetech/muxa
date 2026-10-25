//! Decides each pane's role from tags only, never from screen content.
//! Sources, first match wins:
//! 1. `@muxa_role` (agent | shell | hidden), set by the user or any tool.
//! 2. tmux-agents tags: `@agents_sidebar_pane`, `@agents_role` (ai | term),
//!    and on sessions made before `@agents_role` existed, the pane where an
//!    agent hook ran (`@agent_session_id`, `@agent_state`).
//! 3. Otherwise [`Role::Unknown`].

use std::collections::BTreeMap;

use crate::WindowId;
use crate::discover::{AgentState, AgentStatus, Pane, Role};

/// The tags read for one pane.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PaneTags {
    pub muxa_role: Option<String>,
    pub sidebar: bool,
    pub agents_role: Option<String>,
    pub agent_session_id: Option<String>,
    pub agent_state: Option<String>,
    pub agent_state_at: Option<u64>,
}

const SHELLS: [&str; 8] = ["sh", "bash", "zsh", "fish", "dash", "ksh", "tcsh", "nu"];

pub(crate) fn is_shell(command: &str) -> bool {
    SHELLS.contains(&command.trim_start_matches('-'))
}

fn tagged(tags: &PaneTags) -> Role {
    match tags.muxa_role.as_deref() {
        Some("agent") => return Role::Agent,
        Some("shell") => return Role::Shell,
        Some("hidden") => return Role::Hidden,
        _ => {}
    }
    if tags.sidebar {
        return Role::Hidden;
    }
    match tags.agents_role.as_deref() {
        Some("ai") => Role::Agent,
        Some("term") => Role::Shell,
        _ => Role::Unknown,
    }
}

/// Sets `role` and `agent` on `panes`; `tags[i]` belongs to `panes[i]`.
pub(crate) fn assign(panes: &mut [Pane], tags: &[PaneTags]) {
    for (pane, t) in panes.iter_mut().zip(tags) {
        pane.role = tagged(t);
    }
    let mut windows: BTreeMap<WindowId, Vec<usize>> = BTreeMap::new();
    for (i, pane) in panes.iter().enumerate() {
        windows.entry(pane.window).or_default().push(i);
    }
    for idx in windows.values() {
        infer_legacy_agent(panes, tags, idx);
    }
    for (pane, t) in panes.iter_mut().zip(tags) {
        if pane.role == Role::Agent {
            pane.agent = t
                .agent_state
                .as_deref()
                .and_then(AgentState::parse)
                .map(|state| AgentStatus {
                    state,
                    since: t.agent_state_at,
                });
        }
    }
}

/// A window with no agent pane tagged: the pane where an agent hook ran is the
/// agent (the newest, when hooks ran in several), and its untagged
/// neighbours are shells.
fn infer_legacy_agent(panes: &mut [Pane], tags: &[PaneTags], idx: &[usize]) {
    if idx.iter().any(|&i| panes[i].role == Role::Agent) {
        return;
    }
    let candidate = idx
        .iter()
        .copied()
        .filter(|&i| panes[i].role == Role::Unknown)
        .filter(|&i| tags[i].agent_session_id.is_some() || tags[i].agent_state.is_some())
        .max_by_key(|&i| tags[i].agent_state_at.unwrap_or(0));
    let Some(agent) = candidate else { return };
    panes[agent].role = Role::Agent;
    for &i in idx {
        if panes[i].role == Role::Unknown {
            panes[i].role = Role::Shell;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PaneId;

    fn pane(n: u32, window: u32, command: &str) -> Pane {
        Pane {
            id: PaneId::new(n),
            window: WindowId::new(window),
            index: n,
            role: Role::Unknown,
            cols: 80,
            rows: 24,
            command: command.into(),
            path: String::new(),
            dead: false,
            pid: 0,
            shell_foreground: is_shell(command),
            agent: None,
        }
    }

    fn roles(panes: &[Pane]) -> Vec<Role> {
        panes.iter().map(|p| p.role).collect()
    }

    #[test]
    fn muxa_role_wins() {
        let mut panes = vec![pane(1, 1, "zsh"), pane(2, 1, "zsh"), pane(3, 1, "zsh")];
        let tags = vec![
            PaneTags {
                muxa_role: Some("shell".into()),
                agents_role: Some("ai".into()),
                ..Default::default()
            },
            PaneTags {
                muxa_role: Some("agent".into()),
                ..Default::default()
            },
            PaneTags {
                muxa_role: Some("hidden".into()),
                ..Default::default()
            },
        ];
        assign(&mut panes, &tags);
        assert_eq!(roles(&panes), vec![Role::Shell, Role::Agent, Role::Hidden]);
    }

    #[test]
    fn tmux_agents_roles_and_state() {
        let mut panes = vec![pane(1, 1, "bash"), pane(2, 1, "2.1.0"), pane(3, 1, "zsh")];
        let tags = vec![
            PaneTags {
                sidebar: true,
                ..Default::default()
            },
            PaneTags {
                agents_role: Some("ai".into()),
                agent_state: Some("waiting".into()),
                agent_state_at: Some(1700000000),
                ..Default::default()
            },
            PaneTags {
                agents_role: Some("term".into()),
                ..Default::default()
            },
        ];
        assign(&mut panes, &tags);
        assert_eq!(roles(&panes), vec![Role::Hidden, Role::Agent, Role::Shell]);
        assert_eq!(
            panes[1].agent,
            Some(AgentStatus {
                state: AgentState::Waiting,
                since: Some(1700000000)
            })
        );
        assert_eq!(panes[2].agent, None);
    }

    #[test]
    fn legacy_session_uses_the_newest_hook_pane() {
        // @agent_session_id is never cleared, so an old pane can carry one too.
        let mut panes = vec![pane(1, 1, "bash"), pane(2, 1, "2.1.0"), pane(3, 1, "zsh")];
        let tags = vec![
            PaneTags {
                sidebar: true,
                ..Default::default()
            },
            PaneTags {
                agent_session_id: Some("a".into()),
                agent_state: Some("idle".into()),
                agent_state_at: Some(200),
                ..Default::default()
            },
            PaneTags {
                agent_session_id: Some("b".into()),
                agent_state_at: Some(100),
                ..Default::default()
            },
        ];
        assign(&mut panes, &tags);
        assert_eq!(roles(&panes), vec![Role::Hidden, Role::Agent, Role::Shell]);
    }

    #[test]
    fn untagged_panes_stay_unknown() {
        let mut panes = vec![pane(1, 1, "vim"), pane(2, 1, "zsh")];
        assign(&mut panes, &[PaneTags::default(), PaneTags::default()]);
        assert_eq!(roles(&panes), vec![Role::Unknown, Role::Unknown]);
    }

    #[test]
    fn unknown_agent_states_are_dropped() {
        let mut panes = vec![pane(1, 1, "x")];
        let tags = vec![PaneTags {
            agents_role: Some("ai".into()),
            agent_state: Some("thinking".into()),
            ..Default::default()
        }];
        assign(&mut panes, &tags);
        assert_eq!(panes[0].agent, None);
    }

    #[test]
    fn recognises_login_shells() {
        assert!(is_shell("-zsh") && is_shell("bash") && !is_shell("node"));
    }
}
