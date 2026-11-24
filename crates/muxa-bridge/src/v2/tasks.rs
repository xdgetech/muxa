//! The host's side of the v2 server: a Task's places (`places.rs`, ADR 0068),
//! the Workspace's agent command and environment (ADR 0016), and the
//! Workspaces for Capture and Start Task, and closing or restarting a Task.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use muxa_protocol::v2::{Place, PlaceKind};
use muxa_server::tasks::{BOT, BRANCH, CHECKOUT, DIR, PLACES, REPO, SUBSCRIPTION, WORKSPACE};
use muxa_server::{Host, Prepared, RoutineList, TaskError, TaskSpec};

use super::adopt::{self, AgentLookup};
use super::chat::Chats;
use super::mcp;
use super::places;
use super::runner::Runner;
use super::task_loop::TaskLoops;
use crate::records::Workspace;
use crate::records::mcp::McpServer;
use crate::records::subscriptions::{self, Login};
use crate::records::workers::LaunchCommand;
use crate::routines::runs::{StartError, When};
use crate::settings::{SettingsError, SettingsStore};

pub struct BridgeHost {
    /// `--task-command`: the agent when the Workspace names none.
    command: String,
    records: Arc<crate::config::Config>,
    /// Runs Routines on the session backend, for Run now.
    runner: Option<Arc<Runner>>,
    loops: Option<Arc<TaskLoops>>,
    chats: Option<Arc<Chats>>,
    settings: Option<Arc<SettingsStore>>,
    /// Claude Code's config dirs, for `--continue` in an adopted place.
    /// Empty: never `--continue`.
    claude_dirs: Vec<PathBuf>,
    /// Finds an agent that already works in an adopted place.
    agents_in: Arc<AgentLookup>,
}

impl BridgeHost {
    pub fn new(command: String, records: Arc<crate::config::Config>) -> Self {
        BridgeHost {
            command,
            records,
            runner: None,
            loops: None,
            chats: None,
            settings: None,
            claude_dirs: Vec::new(),
            agents_in: adopt::host_lookup(),
        }
    }

    pub fn with_claude_dirs(self, claude_dirs: Vec<PathBuf>) -> Self {
        BridgeHost {
            claude_dirs,
            ..self
        }
    }

    #[cfg(test)]
    pub(crate) fn with_agent_lookup(self, agents_in: Arc<AgentLookup>) -> Self {
        BridgeHost { agents_in, ..self }
    }

    pub fn with_settings(self, settings: Arc<SettingsStore>) -> Self {
        BridgeHost {
            settings: Some(settings),
            ..self
        }
    }

    fn settings_store(&self) -> Result<Arc<SettingsStore>, TaskError> {
        self.settings
            .clone()
            .ok_or_else(|| TaskError::Invalid("this host has no settings".into()))
    }

    fn task_loops(&self) -> Result<&TaskLoops, TaskError> {
        self.loops
            .as_deref()
            .ok_or_else(|| TaskError::Invalid("this host runs no Task Loops".into()))
    }

    pub fn with_chats(self, chats: Arc<Chats>) -> Self {
        BridgeHost {
            chats: Some(chats),
            ..self
        }
    }

    pub fn with_loops(self, loops: Arc<TaskLoops>) -> Self {
        BridgeHost {
            loops: Some(loops),
            ..self
        }
    }

    pub fn with_runner(self, runner: Arc<Runner>) -> Self {
        BridgeHost {
            runner: Some(runner),
            ..self
        }
    }

    async fn workspace(&self, name: &str) -> Result<Workspace, TaskError> {
        let (records, wanted) = (Arc::clone(&self.records), name.to_owned());
        blocking(move || Ok(records.load_workspaces()?.get(&wanted).cloned()))
            .await?
            .ok_or_else(|| TaskError::Invalid(format!("no Workspace named {name:?}")))
    }

    /// Claude Code kept a conversation for `dir`, so `--continue` resumes it.
    /// With a login, only its folder counts: the agent reads no other.
    fn resumes(&self, ws: &Workspace, line: &str, dir: &Path, login: Option<&Login>) -> bool {
        let dirs = match login {
            Some(l) => vec![l.home.clone()],
            None => adopt::claude_dirs(ws, &self.claude_dirs),
        };
        crate::launch::is_claude_code(&LaunchCommand::Shell(line.to_owned()))
            && crate::agents::claude::has_conversation(&dirs, dir)
    }

    /// `line` with the status line that records `login`'s usage (ADR 0091 §6).
    async fn with_status_line(
        &self,
        line: String,
        dir: &Path,
        login: Option<&Login>,
    ) -> Result<String, TaskError> {
        let hook = login.and_then(|l| {
            crate::status_line::Hook::current(self.records.state_dir.as_deref(), &l.name)
        });
        let Some(hook) = hook else { return Ok(line) };
        let dir = dir.to_owned();
        blocking(move || crate::status_line::add_to_line(line, &dir, &hook)).await
    }

    /// The login an agent started with `line` runs on (ADR 0091): a talk's
    /// Bot list when it has one, else the Workspace's. `keep` wins while it
    /// exists; an adopted place keeps the login that holds its conversation.
    async fn login(
        &self,
        ws: &Workspace,
        bot: Option<&str>,
        line: &str,
        keep: Option<String>,
        adopted: Option<PathBuf>,
    ) -> Result<Option<Login>, TaskError> {
        let (records, ws, bot, line) = (
            Arc::clone(&self.records),
            ws.clone(),
            bot.map(str::to_owned),
            line.to_owned(),
        );
        blocking(move || {
            let all = records.load_workspaces()?;
            let names = bot
                .and_then(|b| all.bot(&ws.name, &b).map(|b| b.subscriptions.clone()))
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| ws.subscriptions.clone());
            let keep = keep.or_else(|| {
                let dir = adopted?;
                subscriptions::holding(all.subscriptions(), &ws.subscriptions, &dir)
            });
            Ok(subscriptions::login(
                all.subscriptions(),
                &names,
                &LaunchCommand::Shell(line),
                keep.as_deref(),
            ))
        })
        .await
    }

    fn agent_command(&self, ws: &Workspace) -> String {
        ws.agent
            .as_ref()
            .and_then(|a| a.command.clone())
            .unwrap_or_else(|| self.command.clone())
    }

    /// The Workspace's agent command line for `made`, with Claude Code's
    /// settings for the other places and, in a workspace that lists MCP
    /// servers, its `.muxa/mcp.json` written beside `TASK.md`. `servers`
    /// is `None` on a restart: the file the Task started with is kept.
    async fn agent_line(
        &self,
        ws: &Workspace,
        made: places::Made,
        servers: Option<Vec<McpServer>>,
        talk: bool,
    ) -> Result<(places::Made, String), TaskError> {
        let command = if talk {
            // A talk is its Bot: Claude Code, whatever the Workspace's agent (ADR 0087).
            match super::bot_run::as_bot(LaunchCommand::Shell(self.agent_command(ws))) {
                LaunchCommand::Shell(line) => line,
                LaunchCommand::Native { .. } => super::bot_run::CLAUDE.to_owned(),
            }
        } else {
            self.agent_command(ws)
        };
        let claude = crate::launch::is_claude_code(&LaunchCommand::Shell(command.clone()));
        // A talk is its Bot: nothing beyond the Workspace's servers loads
        // unless claude.ai connectors are allowed (ADR 0087).
        let managed = mcp::managed(ws) || (talk && !ws.claude_connectors);
        let known = ws.connections.clone();
        let strict = !ws.claude_connectors;
        let (made, flags) = blocking(move || {
            let mut flags = Vec::new();
            if claude {
                flags.extend(places::claude_settings(&made)?);
                if managed {
                    let existing = made.working().join(".muxa").join("mcp.json");
                    let config = match servers {
                        None if existing.is_file() => existing,
                        servers => {
                            let servers = servers.unwrap_or_default();
                            let names: Vec<&str> =
                                servers.iter().map(|s| s.name.as_str()).collect();
                            tracing::info!(mcp = ?names, "mcp servers given");
                            mcp::write(made.working(), &servers, &known)?
                        }
                    };
                    flags.push(mcp::shell_flags(&config, strict));
                }
            }
            Ok((made, flags))
        })
        .await?;
        let line = std::iter::once(command)
            .chain(flags)
            .collect::<Vec<_>>()
            .join(" ");
        Ok((made, line))
    }
}

impl BridgeHost {
    /// The talk part of a talk Task's brief (ADR 0083). The Task is one
    /// folder; the Bot is one of the Workspace's, or a name for a new one.
    async fn talk_brief(
        &self,
        ws: &Workspace,
        bot: &str,
        places: &[muxa_protocol::v2::PlaceSpec],
    ) -> Result<String, TaskError> {
        crate::records::bots::name(bot).map_err(TaskError::Invalid)?;
        if places
            .iter()
            .any(|p| !matches!(p, muxa_protocol::v2::PlaceSpec::Folder))
        {
            return Err(TaskError::Invalid(
                "a talk with a Bot is one folder; send no other places".into(),
            ));
        }
        let store = self.settings_store()?;
        let view = settings_call(move || store.view()).await?;
        let found = view
            .workspaces
            .iter()
            .find(|w| w.name == ws.name)
            .ok_or_else(|| TaskError::Invalid(format!("no Workspace named {:?}", ws.name)))?;
        Ok(super::talk_check::brief(found, bot))
    }

    /// The Bot's own `commands` (ADR 0088); none for a Bot not added yet.
    async fn bot_commands(&self, ws: &Workspace, bot: &str) -> Result<Vec<String>, TaskError> {
        let store = self.settings_store()?;
        let view = settings_call(move || store.view()).await?;
        Ok(view
            .workspaces
            .iter()
            .find(|w| w.name == ws.name)
            .and_then(|w| w.bots.iter().find(|b| b.spec.name == bot))
            .map(|b| b.spec.commands.clone())
            .unwrap_or_default())
    }
}

/// Claude Code may not edit the Workspace's `.muxa/` in a talk: the Bot
/// proposes changes instead (ADR 0083). It is told to read its brief first.
/// The Bot's `commands` are allowed too; the user is in the talk, so anything
/// else still asks (ADR 0088).
async fn talk_line(
    ws: &Workspace,
    dir: &Path,
    line: String,
    commands: Vec<String>,
) -> Result<String, TaskError> {
    let launch = LaunchCommand::Shell(line.clone());
    if !crate::launch::is_claude_code(&launch) {
        return Ok(line);
    }
    let (ws, dir) = (ws.clone(), dir.to_owned());
    blocking(move || {
        let muxa = crate::records::routines::routines_dir(&ws)?
            .unwrap_or_else(|| ws.root.join(crate::records::routines::WORKSPACE_DIR));
        let (rules, allow) =
            super::bot_run::bot_rules(&ws, &commands, vec![super::bot_run::deny(&muxa)], &dir, &[]);
        let prompt = super::talk_check::START_PROMPT;
        match super::bot_run::with_permissions(launch, &dir, &rules, &allow, &[])? {
            LaunchCommand::Shell(line) => Ok(format!(
                "{line} --append-system-prompt '{}'",
                prompt.replace('\'', "'\\''")
            )),
            LaunchCommand::Native { .. } => Ok(line),
        }
    })
    .await
}

/// The places of an ended agent from its labels; its working place must
/// still be there.
fn made_of(ended: &muxa_backend::TerminalInfo, dir: &str) -> Result<places::Made, TaskError> {
    if !std::path::Path::new(dir).is_dir() {
        return Err(TaskError::Invalid(format!(
            "the working place {dir} is gone; start the Task again"
        )));
    }
    let places: Vec<Place> = ended
        .labels
        .get(PLACES)
        .and_then(|p| serde_json::from_str(p).ok())
        .unwrap_or_default();
    let branch = ended.labels.get(BRANCH).cloned();
    let places = match places.first() {
        Some(first) if first.path == dir => places,
        Some(_) => {
            return Err(TaskError::Invalid(format!(
                "{} works in {dir}, which its places do not start with",
                ended.id
            )));
        }
        // An agent started before `muxa.places`: its working place only.
        None => vec![Place {
            kind: if branch.is_some() {
                PlaceKind::Branch
            } else {
                PlaceKind::Folder
            },
            path: dir.to_owned(),
            branch: branch.clone(),
        }],
    };
    let repo = ended.labels.get(REPO).map(PathBuf::from);
    Ok(places::Made {
        places,
        in_repo: repo.is_some(),
        repo,
        branch,
    })
}

/// The secrets of every connection of `ws`, from the Keychain (ADR 0075).
/// A missing or locked one fails.
async fn connection_env(ws: &Workspace) -> Result<Vec<(String, String)>, TaskError> {
    let ws = ws.clone();
    blocking(move || crate::connections::env(&ws)).await
}

/// What every agent of `ws` gets, its secrets read.
async fn given(ws: &Workspace) -> Result<Given, TaskError> {
    let (connections, mcp) = crate::usage::given(ws);
    Ok(Given {
        env: connection_env(ws).await?,
        connections,
        mcp,
    })
}

static CHOICE_IGNORED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// What the agent gets of its Workspace: all its connections and MCP servers.
struct Given {
    connections: Vec<String>,
    mcp: Vec<String>,
    env: Vec<(String, String)>,
}

fn prepared(
    ws: Workspace,
    title: &str,
    made: places::Made,
    line: String,
    given: Given,
    login: Option<Login>,
) -> Prepared {
    if let Some(a) = &ws.agent {
        // Keys only: values may be tokens.
        tracing::info!(keys = ?a.keys(), "agent environment");
    }
    Prepared {
        dir: made.working().to_owned(),
        workspace: ws.name.clone(),
        title: title.to_owned(),
        repo: made.repo.clone(),
        branch: made.branch.clone(),
        checkout: None,
        places: made.places,
        connections: given.connections,
        mcp: given.mcp,
        argv: shell_line(line),
        bot: None,
        subscription: login.as_ref().map(|l| l.name.clone()),
        env: {
            let mut env = ws.agent.map(|a| a.env).unwrap_or_default();
            crate::connections::merge(&mut env, given.env);
            // The loader refuses a login variable beside subscriptions.
            env.extend(login.and_then(|l| l.env));
            env.into_iter().collect()
        },
    }
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, TaskError> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| TaskError::Failed(e.to_string()))?
        .map_err(TaskError::Failed)
}

/// Runs a settings call off the async threads.
async fn settings_call<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, SettingsError> + Send + 'static,
) -> Result<T, TaskError> {
    let done = tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| TaskError::Failed(e.to_string()))?;
    done.map_err(|e| match e {
        SettingsError::Invalid(m) => TaskError::Invalid(m),
        SettingsError::Stale => TaskError::Stale(e.to_string()),
        SettingsError::ReadOnly => TaskError::ReadOnly(e.to_string()),
        SettingsError::Failed(m) => {
            tracing::warn!(error = %m, "settings not changed");
            TaskError::Failed(m)
        }
    })
}

impl Host for BridgeHost {
    fn shell(&self) -> Vec<String> {
        login_shell(std::env::var("SHELL").ok().as_deref())
    }

    async fn settings(&self) -> Result<muxa_protocol::v2::Settings, TaskError> {
        let store = self.settings_store()?;
        settings_call(move || store.view()).await
    }

    async fn settings_update(
        &self,
        revision: &str,
        ops: Vec<muxa_protocol::v2::SettingsOp>,
    ) -> Result<muxa_protocol::v2::Settings, TaskError> {
        let (store, revision) = (self.settings_store()?, revision.to_owned());
        let updated = settings_call(move || store.update(&revision, ops)).await?;
        if let Some(chats) = &self.chats {
            chats.reload_agents().await;
        }
        Ok(updated)
    }

    async fn prepare(&self, spec: &TaskSpec) -> Result<Prepared, TaskError> {
        let title = spec.title.trim();
        if title.is_empty() {
            return Err(TaskError::Invalid("a Task needs a title".into()));
        }
        let ws = self.workspace(&spec.workspace).await?;
        let checked = places::check(&ws, &spec.places).await?;
        if let Some(a) = places::adopted(&checked) {
            adopt::refuse_if_busy(Arc::clone(&self.agents_in), &a.path).await?;
        }
        if !spec.connections.is_empty() || !spec.mcp.is_empty() {
            crate::records::ignored_once(
                &CHOICE_IGNORED,
                "start-task connections and mcp are no longer used: a Task gets all its workspace's connections and MCP servers (ADR 0075)",
            );
        }
        let servers = mcp::chosen(&ws);
        let launch = LaunchCommand::Shell(self.agent_command(&ws));
        if spec.bot.is_none() && !crate::launch::is_claude_code(&launch) {
            mcp::only_claude(crate::launch::program(&launch), &servers)
                .map_err(TaskError::Invalid)?;
        }
        let talk = match &spec.bot {
            Some(bot) => Some(self.talk_brief(&ws, bot, &spec.places).await?),
            None => None,
        };
        // Before any place is made: a missing secret stops the start.
        let given = given(&ws).await?;
        let made = places::make(&ws, title, checked).await?;
        let (title_owned, goal) = (title.to_owned(), spec.goal.clone());
        let talk_text = talk.clone();
        let made = blocking(move || {
            places::brief(&made, &title_owned, goal.as_deref())?;
            if let Some(text) = &talk_text {
                super::git::append_file(made.working(), "TASK.md", text)?;
            }
            Ok(made)
        })
        .await?;
        let (made, mut line) = self
            .agent_line(&ws, made, Some(servers), talk.is_some())
            .await?;
        if talk.is_some() {
            let commands = match &spec.bot {
                Some(bot) => self.bot_commands(&ws, bot).await?,
                None => Vec::new(),
            };
            line = talk_line(&ws, made.working(), line, commands).await?;
        }
        let adopts = spec.existing().is_some() || spec.checkout().is_some();
        let adopted = adopts.then(|| made.working().to_owned());
        let login = self
            .login(&ws, spec.bot.as_deref(), &line, None, adopted)
            .await?;
        if adopts && self.resumes(&ws, &line, made.working(), login.as_ref()) {
            line.push_str(" --continue");
        }
        let mut line = self
            .with_status_line(line, made.working(), login.as_ref())
            .await?;
        if spec.plan {
            line = crate::launch::plan_line(&line);
        }
        tracing::info!(workspace = %ws.name, places = made.places.len(), "task places ready");
        let mut prepared = prepared(ws, title, made, line, given, login);
        prepared.checkout = spec.checkout().map(str::to_owned);
        prepared.bot = spec.bot.clone();
        Ok(prepared)
    }

    async fn restart(&self, ended: &muxa_backend::TerminalInfo) -> Result<Prepared, TaskError> {
        let label = |key: &str| {
            ended.labels.get(key).cloned().ok_or_else(|| {
                TaskError::Invalid(format!(
                    "{} has no {key} label; start the Task again",
                    ended.id
                ))
            })
        };
        let ws = self.workspace(&label(WORKSPACE)?).await?;
        let made = made_of(ended, &label(DIR)?)?;
        // The Workspace's connections and servers as they are now.
        let given = given(&ws).await?;
        let is_talk = ended.labels.contains_key(BOT);
        let (made, mut line) = self
            .agent_line(&ws, made, Some(mcp::chosen(&ws)), is_talk)
            .await?;
        if is_talk {
            let commands = match ended.labels.get(BOT) {
                Some(bot) => self.bot_commands(&ws, bot).await?,
                None => Vec::new(),
            };
            line = talk_line(&ws, made.working(), line, commands).await?;
        }
        let title = ended
            .name
            .clone()
            .or_else(|| made.branch.clone())
            .unwrap_or_else(|| "Task".into());
        // The conversation lives in the ended agent's login (ADR 0091 §5).
        let keep = ended.labels.get(SUBSCRIPTION).cloned();
        let bot = ended.labels.get(BOT).cloned();
        let login = self.login(&ws, bot.as_deref(), &line, keep, None).await?;
        let line = self
            .with_status_line(line, made.working(), login.as_ref())
            .await?;
        let mut prepared = prepared(ws, &title, made, line, given, login);
        prepared.checkout = ended.labels.get(CHECKOUT).cloned();
        prepared.bot = ended.labels.get(BOT).cloned();
        Ok(prepared)
    }

    async fn suggestion_decided(
        &self,
        item: &muxa_protocol::v2::InboxItem,
        suggestion: &muxa_protocol::v2::ItemSuggestion,
        accepted: bool,
    ) {
        let (records, item, suggestion) =
            (Arc::clone(&self.records), item.clone(), suggestion.clone());
        let noted = tokio::task::spawn_blocking(move || {
            super::assistant::note_decision(&records, &item, &suggestion, accepted)
        })
        .await;
        if let Ok(Err(e)) = noted {
            tracing::warn!(error = %e, "the assistant's decision was not kept");
        }
    }

    async fn v1_tasks(&self) -> Result<Vec<muxa_protocol::v2::V1Task>, TaskError> {
        let records = Arc::clone(&self.records);
        let workspaces = blocking(move || records.load_workspaces()).await?;
        super::v1::tasks(self.records.state_dir.as_deref(), &workspaces).await
    }

    async fn agent_started(&self, prepared: &Prepared) {
        let ws = match self.workspace(&prepared.workspace).await {
            Ok(ws) => ws,
            Err(e) => {
                tracing::warn!(error = %e, "cannot note when connections were used");
                return;
            }
        };
        let (connections, mcp) = crate::usage::given(&ws);
        let state_dir = self.records.state_dir.clone();
        let noted = tokio::task::spawn_blocking(move || {
            crate::usage::note(state_dir.as_deref(), &ws.name, &connections, &mcp);
        })
        .await;
        if let Err(e) = noted {
            tracing::warn!(error = %e, "cannot note when connections were used");
        }
    }

    async fn login_launch(
        &self,
        subscription: &str,
    ) -> Result<muxa_server::LoginLaunch, TaskError> {
        let (records, name) = (Arc::clone(&self.records), subscription.to_owned());
        let shell = self.shell();
        let all = blocking(move || records.load_workspaces()).await?;
        let sub = all
            .subscriptions()
            .iter()
            .find(|s| s.name == name)
            .ok_or_else(|| TaskError::Invalid(format!("no subscription named {name:?}")))?;
        Ok(crate::status_line::login_launch(
            sub,
            shell,
            self.records.home.as_deref(),
        ))
    }

    async fn subscriptions(&self) -> Result<Vec<muxa_protocol::v2::SubscriptionUsage>, TaskError> {
        let records = Arc::clone(&self.records);
        blocking(move || {
            let all = records.load_workspaces()?;
            Ok(all
                .subscriptions()
                .iter()
                .map(|s| {
                    let found = match &records.state_dir {
                        Some(dir) => crate::limits::read(dir, &s.name).unwrap_or_else(|e| {
                            tracing::info!(subscription = %s.name, error = %e, "limits not read");
                            crate::limits::Record::default()
                        }),
                        None => crate::limits::Record::default(),
                    };
                    crate::limits::usage(s, &found)
                })
                .collect())
        })
        .await
    }

    async fn workspace_list(&self) -> Result<Vec<muxa_protocol::v2::Workspace>, TaskError> {
        let (records, command) = (Arc::clone(&self.records), self.command.clone());
        blocking(move || {
            let all = records.load_workspaces()?;
            let usage = match &records.state_dir {
                Some(dir) => crate::usage::read(dir).unwrap_or_else(|e| {
                    tracing::warn!(error = %e, "cannot read when connections were used");
                    crate::usage::Usage::default()
                }),
                None => crate::usage::Usage::default(),
            };
            Ok(all
                .all()
                .iter()
                .map(|w| {
                    let routines: Vec<_> = all
                        .routines()
                        .iter()
                        .filter(|r| r.workspace == w.name)
                        .collect();
                    super::workspaces::to_v2(w, &command, &routines, &usage.of(&w.name))
                })
                .collect())
        })
        .await
    }

    async fn check_close(
        &self,
        places: &[muxa_protocol::v2::Place],
        mode: muxa_protocol::v2::WorktreeMode,
    ) -> Result<(), TaskError> {
        super::close::check(places, mode).await
    }

    async fn close_places(
        &self,
        places: &[muxa_protocol::v2::Place],
        mode: muxa_protocol::v2::WorktreeMode,
    ) -> Result<(), TaskError> {
        super::close::remove(places, mode).await
    }

    async fn routines(&self) -> Result<RoutineList, TaskError> {
        let records = Arc::clone(&self.records);
        let listed = tokio::task::spawn_blocking(move || crate::routines::list(&records))
            .await
            .map_err(|e| TaskError::Failed(e.to_string()))?;
        let listed = listed.map_err(TaskError::Failed)?;
        Ok(RoutineList {
            routines: listed
                .routines
                .into_iter()
                .map(super::routines::to_v2)
                .collect(),
            problems: listed
                .problems
                .into_iter()
                .map(super::routines::problem)
                .collect(),
            off: listed
                .off
                .into_iter()
                .map(|(workspace, count)| muxa_protocol::v2::RoutinesOff { workspace, count })
                .collect(),
        })
    }

    async fn run_routine<B: muxa_backend::SessionBackend>(
        &self,
        backend: &B,
        workspace: &str,
        name: &str,
    ) -> Result<(), TaskError> {
        let runner = self
            .runner
            .as_ref()
            .ok_or_else(|| TaskError::Invalid("this host runs no Routines".into()))?;
        runner
            .start(backend, workspace, name, When::Now(crate::records::now()))
            .await
            .map(drop)
            .map_err(|e| match e {
                StartError::Bad(m) | StartError::Unsupported(m) => TaskError::Invalid(m),
                StartError::Busy(m) => TaskError::Busy(m),
                StartError::Handled => TaskError::Busy(e.to_string()),
                StartError::Failed(m) => TaskError::Failed(m),
            })
    }

    async fn loop_check(
        &self,
        workspace: &str,
        spec: &muxa_protocol::v2::LoopSpec,
    ) -> Result<(), TaskError> {
        self.task_loops()?.resolve(workspace, spec).await.map(drop)
    }

    async fn loop_start(
        &self,
        agent: &muxa_backend::TerminalInfo,
        spec: &muxa_protocol::v2::LoopSpec,
        reset: bool,
    ) -> Result<muxa_protocol::v2::LoopInfo, TaskError> {
        self.task_loops()?.start(agent, spec, reset).await
    }

    async fn loop_stop(&self, terminal: &str) -> Result<muxa_protocol::v2::LoopInfo, TaskError> {
        self.task_loops()?.stop(terminal).await
    }

    async fn loops(&self) -> Vec<muxa_protocol::v2::LoopInfo> {
        match &self.loops {
            Some(l) => l.list().await,
            None => Vec::new(),
        }
    }

    fn loop_changes(&self) -> Option<tokio::sync::watch::Receiver<u64>> {
        self.loops.as_ref().map(|l| l.subscribe())
    }

    async fn chat_open(
        &self,
        dir: &std::path::Path,
    ) -> Result<(u64, muxa_protocol::v2::ChatUpdate), TaskError> {
        match &self.chats {
            Some(c) => c.open(dir).await,
            None => Err(TaskError::Invalid("this host has no Chat".into())),
        }
    }

    async fn chat_poll(
        &self,
        handle: u64,
    ) -> Result<Option<muxa_protocol::v2::ChatUpdate>, TaskError> {
        match &self.chats {
            Some(c) => c.poll(handle).await,
            None => Err(TaskError::Invalid("this host has no Chat".into())),
        }
    }

    async fn chat_close(&self, handle: u64) {
        if let Some(c) = &self.chats {
            c.close(handle);
        }
    }

    async fn reader_list(
        &self,
        dir: &std::path::Path,
        kind: muxa_protocol::v2::ReaderKind,
        html: bool,
    ) -> Result<muxa_protocol::v2::ReaderList, TaskError> {
        super::reader::list(dir, kind, html).await
    }

    async fn upload(
        &self,
        dir: &std::path::Path,
        name: &str,
        bytes: Vec<u8>,
    ) -> Result<String, TaskError> {
        super::uploads::write(dir, name, bytes).await
    }

    async fn reader_read(
        &self,
        dir: &std::path::Path,
        open: &muxa_server::ReaderOpen,
    ) -> Result<muxa_protocol::v2::ReaderContent, TaskError> {
        super::reader::read(dir, open).await
    }

    async fn workspaces(&self) -> Vec<String> {
        let records = Arc::clone(&self.records);
        let loaded = tokio::task::spawn_blocking(move || records.load_workspaces()).await;
        match loaded {
            Ok(Ok(w)) => w.all().iter().map(|w| w.name.clone()).collect(),
            Ok(Err(_)) | Err(_) => Vec::new(),
        }
    }
}

/// The agent command is a shell line, as tmux ran it.
fn shell_line(line: String) -> Vec<String> {
    vec!["/bin/sh".into(), "-c".into(), line]
}

#[cfg(test)]
#[path = "resume_tests.rs"]
mod resume_tests;

/// A Task's shell: the user's login shell. A daemon that launchd started may
/// have no `SHELL`, so it falls back to the system's default shell.
fn login_shell(env: Option<&str>) -> Vec<String> {
    let fallback = if cfg!(target_os = "macos") {
        "/bin/zsh"
    } else {
        "/bin/sh"
    };
    let shell = env.filter(|s| s.starts_with('/')).unwrap_or(fallback);
    vec![shell.to_owned(), "-l".to_owned()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_workspace_that_is_not_there_is_invalid() {
        let host = BridgeHost::new(
            "claude".into(),
            Arc::new(crate::config::Config {
                state_dir: None,
                workspaces: None,
                home: None,
            }),
        );
        let spec = TaskSpec {
            workspace: "nowhere".into(),
            title: "Fix".into(),
            goal: None,
            places: Vec::new(),
            connections: Vec::new(),
            mcp: Vec::new(),
            plan: false,
            bot: None,
        };
        assert!(matches!(
            host.prepare(&spec).await,
            Err(TaskError::Invalid(_))
        ));
    }

    /// A host whose one Workspace `w` has `extra` in its table.
    fn host(name: &str, extra: &str) -> (BridgeHost, std::path::PathBuf) {
        let base = std::env::temp_dir().join(format!("muxa-tasks-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("root")).unwrap();
        let base = base.canonicalize().unwrap();
        std::fs::write(
            base.join("workspaces.toml"),
            format!(
                "[[workspace]]\nname = \"w\"\nroot = \"{}\"\n{extra}\
                 [[workspace.mcp]]\nname = \"docs\"\ncommand = \"docs-mcp\"\n",
                base.join("root").display()
            ),
        )
        .unwrap();
        let config = crate::config::Config {
            state_dir: Some(base.join("state")),
            workspaces: Some(base.join("workspaces.toml")),
            home: None,
        };
        (BridgeHost::new("claude".into(), Arc::new(config)), base)
    }

    fn spec(mcp: &[&str]) -> TaskSpec {
        TaskSpec {
            workspace: "w".into(),
            title: "Fix".into(),
            goal: None,
            places: Vec::new(),
            connections: Vec::new(),
            mcp: mcp.iter().map(|m| (*m).to_owned()).collect(),
            plan: false,
            bot: None,
        }
    }

    #[tokio::test]
    async fn a_talk_tells_claude_code_to_read_its_brief_first() {
        let (host, base) = host("talk", "");
        let host = host.with_settings(Arc::new(crate::settings::SettingsStore::new(
            Some(base.join("workspaces.toml")),
            None,
            false,
            Box::<crate::settings::secrets::Fake>::default(),
        )));
        let talk = TaskSpec {
            bot: Some("scout".into()),
            ..spec(&[])
        };
        let ready = host.prepare(&talk).await.unwrap();
        assert!(
            ready.argv[2].ends_with(&format!(
                " --append-system-prompt '{}'",
                super::super::talk_check::START_PROMPT
            )),
            "{}",
            ready.argv[2]
        );
        assert!(super::super::talk_check::START_PROMPT.contains(".muxa/TASK.md"));
        let brief = std::fs::read_to_string(ready.dir.join(".muxa/TASK.md")).unwrap();
        assert!(brief.contains("`.muxa/bot.json`"));
        let plain = host.prepare(&spec(&[])).await.unwrap();
        assert!(!plain.argv[2].contains("--append-system-prompt"));
        let _ = std::fs::remove_dir_all(base);
    }

    #[tokio::test]
    async fn claude_code_gets_every_mcp_server_strictly_and_a_named_one_is_ignored() {
        let (host, base) = host("mcp", "");
        let ready = host.prepare(&spec(&[])).await.unwrap();
        assert_eq!(ready.mcp, ["docs"]);
        let config = ready.dir.join(".muxa/mcp.json");
        assert_eq!(
            ready.argv[2],
            format!(
                "claude --mcp-config '{}' --strict-mcp-config",
                config.display()
            )
        );
        let text = std::fs::read_to_string(&config).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["mcpServers"]["docs"]["command"], "docs-mcp");
        // A name from an older client neither fails nor narrows (ADR 0075).
        let named = host.prepare(&spec(&["web"])).await.unwrap();
        assert_eq!(named.mcp, ["docs"]);
        let _ = std::fs::remove_dir_all(base);
    }

    #[tokio::test]
    async fn a_started_agent_notes_when_its_mcp_servers_were_used() {
        let (host, base) = host("usage", "");
        let ready = host.prepare(&spec(&[])).await.unwrap();
        assert_eq!(ready.mcp, ["docs"]);
        let before = host.workspace_list().await.unwrap();
        assert_eq!(before[0].mcp[0].last_used, None);
        let at = crate::records::now();
        host.agent_started(&ready).await;
        let after = host.workspace_list().await.unwrap();
        assert!(after[0].mcp[0].last_used.is_some_and(|t| t >= at));
        let text = std::fs::read_to_string(base.join("state/usage.json")).unwrap();
        assert!(!text.contains("docs-mcp"), "names only: {text}");
        let _ = std::fs::remove_dir_all(base);
    }

    #[tokio::test]
    async fn a_codex_task_with_mcp_servers_is_refused_before_its_places() {
        let (host, base) = host("codex", "[workspace.agent]\ncommand = \"codex\"\n");
        // The Workspace's servers go to every agent, so naming none is refused too.
        for named in [&[][..], &["docs"][..]] {
            let refused = host.prepare(&spec(named)).await;
            assert!(
                matches!(&refused, Err(TaskError::Invalid(m)) if m.contains("need Claude Code")),
                "{refused:?}"
            );
        }
        assert!(!base.join("root/tasks").exists(), "no place was made");
        let _ = std::fs::remove_dir_all(base);
    }

    /// A Workspace `w` whose agent is `claude --x`, with one Task folder.
    fn host_with_task(name: &str) -> (BridgeHost, PathBuf) {
        let base = std::env::temp_dir().join(format!("muxa-tasks-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("root/tasks/fix-t-1")).unwrap();
        let base = base.canonicalize().unwrap();
        std::fs::write(
            base.join("workspaces.toml"),
            format!(
                "[[workspace]]\nname = \"w\"\nroot = \"{}\"\n[workspace.agent]\ncommand = \"claude --x\"\n",
                base.join("root").display()
            ),
        )
        .unwrap();
        let config = crate::config::Config {
            state_dir: None,
            workspaces: Some(base.join("workspaces.toml")),
            home: None,
        };
        (BridgeHost::new("codex".into(), Arc::new(config)), base)
    }

    #[tokio::test]
    async fn an_ended_agent_restarts_where_it_worked_with_its_places() {
        let (host, base) = host_with_task("restart");
        let dir = base.join("root/tasks/fix-t-1");
        let read = base.join("root");
        let places = serde_json::json!([
            {"kind": "folder", "path": dir},
            {"kind": "read", "path": read},
        ]);
        let ended = muxa_backend::TerminalInfo {
            id: muxa_backend::TerminalId::new("t9"),
            name: Some("Fix login".into()),
            live: false,
            labels: muxa_backend::Labels::from([
                (WORKSPACE.to_owned(), "w".to_owned()),
                (DIR.to_owned(), dir.display().to_string()),
                (PLACES.to_owned(), places.to_string()),
                (muxa_server::tasks::MCP.to_owned(), r#"["docs"]"#.to_owned()),
            ]),
            status: None,
            size: muxa_backend::Size { cols: 80, rows: 24 },
            exit: None,
        };
        let p = host.restart(&ended).await.unwrap();
        assert_eq!(
            (p.dir.as_path(), p.title.as_str()),
            (dir.as_path(), "Fix login")
        );
        assert_eq!(p.places.len(), 2);
        assert!(
            p.mcp.is_empty(),
            "the Workspace's servers now, not the old label"
        );
        let line = &p.argv[2];
        assert!(line.starts_with("claude --x --settings "), "{line}");
        assert!(dir.join(".muxa/claude-settings.json").is_file());

        std::fs::remove_dir_all(&dir).unwrap();
        assert!(matches!(
            host.restart(&ended).await,
            Err(TaskError::Invalid(m)) if m.contains("is gone")
        ));
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[tokio::test]
    async fn workspaces_are_listed_from_the_records() {
        let (host, base) = host_with_task("list");
        let list = host.workspace_list().await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(
            (list[0].name.as_str(), list[0].agent.as_deref()),
            ("w", Some("claude"))
        );
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn the_command_runs_through_the_shell() {
        assert_eq!(
            shell_line("claude --x".into()),
            vec!["/bin/sh", "-c", "claude --x"]
        );
    }

    #[test]
    fn a_tasks_shell_is_the_users_login_shell() {
        assert_eq!(
            login_shell(Some("/opt/homebrew/bin/fish")),
            ["/opt/homebrew/bin/fish", "-l"]
        );
        let fallback = if cfg!(target_os = "macos") {
            "/bin/zsh"
        } else {
            "/bin/sh"
        };
        assert_eq!(login_shell(None), [fallback, "-l"]);
        assert_eq!(
            login_shell(Some("fish")),
            [fallback, "-l"],
            "only an absolute path"
        );
    }
}
