//! The ops of `settings-update` on the files' TOML, in place: keys keep
//! their order and comments, and only changed values are rewritten. Each op
//! checks only what it needs to find its place; `mod.rs` then checks the
//! result with the loading rules.

use std::collections::BTreeMap;

use muxa_protocol::settings::{
    Access, BotSpec, ConnectionSpec, McpSpec, RoutineSpec, SettingsOp, WorkerSpec, WriteOnly,
};
use muxa_protocol::v2::LoopChecks;
use toml_edit::{Array, ArrayOfTables, DocumentMut, InlineTable, Item, Table, TableLike, value};

use super::secrets;

/// What the ops change besides the TOML: secrets and instruction files.
#[derive(Default)]
pub struct Effects {
    /// Keychain account and secret, in op order.
    pub secrets: Vec<(String, WriteOnly)>,
    /// Keychain accounts whose secret goes once the files are written.
    pub forget: Vec<String>,
    /// Workspace, path relative to `.muxa/`, and the new content.
    pub instructions: Vec<(String, String, String)>,
    /// Workspace, Bot and the kept memory to bring back (ADR 0082).
    pub memory_restores: Vec<(String, String, u32)>,
    /// The Capture assistant's new Soul (ADR 0084).
    pub assistant_soul: Option<String>,
    /// Workspace, old and new name of each renamed Bot, for its folder.
    pub bot_renames: Vec<(String, String, String)>,
}

pub fn account(workspace: &str, name: &str) -> String {
    crate::connections::account(workspace, name)
}

/// Applies `op`. A routine op changes the routines document of its Workspace.
pub fn apply(
    workspaces: &mut DocumentMut,
    routines: &mut dyn FnMut(&str) -> Result<DocumentMut, String>,
    changed_routines: &mut BTreeMap<String, DocumentMut>,
    effects: &mut Effects,
    op: SettingsOp,
) -> Result<(), String> {
    match op {
        SettingsOp::WorkspaceAdd { name, root, repos } => {
            if find_ws(workspaces, &name).is_ok() {
                return Err(format!("a Workspace named {name:?} exists already"));
            }
            let mut t = Table::new();
            t.insert("name", value(name));
            t.insert("root", value(root));
            set_list(&mut t, "repos", &repos);
            aot(workspaces.as_table_mut(), "workspace")?.push(t);
            Ok(())
        }
        SettingsOp::WorkspaceUpdate {
            workspace,
            root,
            repos,
            hidden_repos,
            agent_command,
            park_after,
            claude_connectors,
            bot_rules,
            build_paths,
        } => {
            let t = find_ws(workspaces, &workspace)?;
            if let Some(paths) = build_paths {
                let build = sub_table(t, "build")?;
                // The defaults stay unwritten, so they follow muxa's.
                let defaults = paths == super::view::default_build_paths();
                set_opt_list(build, "paths", (!defaults).then_some(&paths));
                drop_empty(t, "build");
            }
            if let Some(on) = claude_connectors {
                set_bool(t, "claude_connectors", on, false);
            }
            if let Some(rules) = bot_rules {
                let bots = sub_table(t, "bots")?;
                // The defaults stay unwritten, so they follow muxa's; an empty
                // list is written, since it means "allow nothing" (ADR 0090).
                let defaults = rules.allow == crate::records::bots::default_allow();
                set_opt_list(bots, "allow", (!defaults).then_some(&rules.allow));
                set_list(bots, "deny", &rules.deny);
                drop_empty(t, "bots");
            }
            if let Some(hidden) = hidden_repos {
                set_list(t, "hidden_repos", &hidden);
            }
            if let Some(root) = root {
                set_str(t, "root", Some(&root));
            }
            if let Some(repos) = repos {
                set_list(t, "repos", &repos);
            }
            if let Some(park) = park_after {
                set_str(t, "park_after", park.as_deref());
            }
            if let Some(command) = agent_command {
                let agent = sub_table(t, "agent")?;
                set_str(agent, "command", command.as_deref());
                drop_empty(t, "agent");
            }
            Ok(())
        }
        SettingsOp::WorkspaceRemove { workspace } => {
            let list = aot(workspaces.as_table_mut(), "workspace")?;
            let i = (0..list.len())
                .find(|&i| {
                    list.get(i)
                        .is_some_and(|t| str_of(t, "name") == Some(&workspace))
                })
                .ok_or_else(|| no_ws(&workspace))?;
            if let Some(t) = list.get(i) {
                for c in names_in(t, "connection") {
                    effects.forget.push(account(&workspace, &c));
                }
            }
            list.remove(i);
            Ok(())
        }
        SettingsOp::EnvSet {
            workspace,
            key,
            value: v,
        } => {
            let t = find_ws(workspaces, &workspace)?;
            let env = sub_table(sub_table(t, "agent")?, "env")?;
            if env.get(&key).and_then(Item::as_str) != Some(v.expose()) {
                env.insert(&key, value(v.expose()));
            }
            Ok(())
        }
        SettingsOp::EnvUnset { workspace, key } => {
            let t = find_ws(workspaces, &workspace)?;
            let removed = t
                .get_mut("agent")
                .and_then(Item::as_table_like_mut)
                .and_then(|a| a.get_mut("env"))
                .and_then(Item::as_table_like_mut)
                .and_then(|env| env.remove(&key));
            if removed.is_none() {
                return Err(format!("[workspace.agent] env has no {key}"));
            }
            if let Some(agent) = t.get_mut("agent").and_then(Item::as_table_like_mut) {
                drop_empty(agent, "env");
            }
            drop_empty(t, "agent");
            Ok(())
        }
        SettingsOp::LoopChecks { workspace, checks } => {
            let t = find_ws(workspaces, &workspace)?;
            match checks {
                None => {
                    t.remove("loop");
                }
                Some(checks) => {
                    let lp = sub_table(t, "loop")?;
                    let item = match checks {
                        LoopChecks::Any => value("any"),
                        LoopChecks::List(list) => value(Array::from_iter(list)),
                    };
                    lp.insert("checks", item);
                }
            }
            Ok(())
        }
        SettingsOp::RoutinesGate {
            workspace,
            enabled,
            direct_writes,
        } => {
            let t = find_ws(workspaces, &workspace)?;
            let gate = sub_table(t, "routines")?;
            set_bool(gate, "enabled", enabled, true);
            set_bool(gate, "direct_writes", direct_writes, false);
            Ok(())
        }
        SettingsOp::ConnectionAdd {
            workspace,
            connection,
            secret,
        } => {
            let t = find_ws(workspaces, &workspace)?;
            if find_named(t, "connection", &connection.name)?.is_some() {
                return Err(format!("connection {:?} exists already", connection.name));
            }
            let mut c = Table::new();
            c.insert("name", value(connection.name.clone()));
            write_connection(&mut c, &connection);
            aot(t, "connection")?.push(c);
            keep_secret(effects, &workspace, &connection.name, secret)
        }
        SettingsOp::ConnectionUpdate {
            workspace,
            connection,
            secret,
        } => {
            let t = find_ws(workspaces, &workspace)?;
            let c = find_named(t, "connection", &connection.name)?
                .ok_or_else(|| format!("there is no connection {:?}", connection.name))?;
            write_connection(c, &connection);
            keep_secret(effects, &workspace, &connection.name, secret)
        }
        SettingsOp::ConnectionSecret {
            workspace,
            name,
            secret,
        } => {
            let t = find_ws(workspaces, &workspace)?;
            if find_named(t, "connection", &name)?.is_none() {
                return Err(format!("there is no connection {name:?}"));
            }
            keep_secret(effects, &workspace, &name, Some(secret))
        }
        SettingsOp::ConnectionRemove { workspace, name } => {
            let t = find_ws(workspaces, &workspace)?;
            remove_named(t, "connection", &name)?;
            effects.forget.push(account(&workspace, &name));
            Ok(())
        }
        SettingsOp::McpAdd { workspace, mcp } => {
            let t = find_ws(workspaces, &workspace)?;
            if find_named(t, "mcp", &mcp.name)?.is_some() {
                return Err(format!("mcp {:?} exists already", mcp.name));
            }
            let mut m = Table::new();
            m.insert("name", value(mcp.name.clone()));
            write_mcp(&mut m, &mcp);
            aot(t, "mcp")?.push(m);
            Ok(())
        }
        SettingsOp::McpUpdate { workspace, mcp } => {
            let t = find_ws(workspaces, &workspace)?;
            let m = find_named(t, "mcp", &mcp.name)?
                .ok_or_else(|| format!("there is no mcp {:?}", mcp.name))?;
            write_mcp(m, &mcp);
            Ok(())
        }
        SettingsOp::McpRemove { workspace, name } => {
            let t = find_ws(workspaces, &workspace)?;
            remove_named(t, "mcp", &name)
        }
        SettingsOp::RoutineAdd {
            workspace,
            routine,
            instructions_text,
        } => {
            find_ws(workspaces, &workspace)?;
            let doc = routines_doc(routines, changed_routines, &workspace)?;
            if find_named(doc.as_table_mut(), "routine", &routine.name)?.is_some() {
                return Err(format!("routine {:?} exists already", routine.name));
            }
            let mut r = Table::new();
            r.insert("name", value(routine.name.clone()));
            write_routine(&mut r, &routine)?;
            aot(doc.as_table_mut(), "routine")?.push(r);
            keep_instructions(effects, &workspace, &routine, instructions_text)
        }
        SettingsOp::RoutineUpdate {
            workspace,
            routine,
            instructions_text,
        } => {
            find_ws(workspaces, &workspace)?;
            let doc = routines_doc(routines, changed_routines, &workspace)?;
            let r = find_named(doc.as_table_mut(), "routine", &routine.name)?
                .ok_or_else(|| format!("there is no routine {:?}", routine.name))?;
            write_routine(r, &routine)?;
            keep_instructions(effects, &workspace, &routine, instructions_text)
        }
        SettingsOp::RoutineRemove { workspace, name } => {
            find_ws(workspaces, &workspace)?;
            let doc = routines_doc(routines, changed_routines, &workspace)?;
            remove_named(doc.as_table_mut(), "routine", &name)
        }
        SettingsOp::BotAdd {
            workspace,
            bot,
            soul_text,
        } => {
            find_ws(workspaces, &workspace)?;
            let doc = routines_doc(routines, changed_routines, &workspace)?;
            if find_named(doc.as_table_mut(), "bot", &bot.name)?.is_some() {
                return Err(format!("bot {:?} exists already", bot.name));
            }
            let mut t = Table::new();
            t.insert("name", value(bot.name.clone()));
            write_bot(&mut t, &bot);
            aot(doc.as_table_mut(), "bot")?.push(t);
            keep_soul(effects, &workspace, &bot.name, soul_text)
        }
        SettingsOp::BotUpdate {
            workspace,
            bot,
            soul_text,
        } => {
            find_ws(workspaces, &workspace)?;
            let doc = routines_doc(routines, changed_routines, &workspace)?;
            let t = find_named(doc.as_table_mut(), "bot", &bot.name)?
                .ok_or_else(|| format!("there is no bot {:?}", bot.name))?;
            write_bot(t, &bot);
            keep_soul(effects, &workspace, &bot.name, soul_text)
        }
        SettingsOp::BotRemove { workspace, name } => {
            find_ws(workspaces, &workspace)?;
            let doc = routines_doc(routines, changed_routines, &workspace)?;
            remove_named(doc.as_table_mut(), "bot", &name)
        }
        SettingsOp::BotRename {
            workspace,
            from,
            to,
        } => {
            find_ws(workspaces, &workspace)?;
            crate::records::bots::name(&to)?;
            let doc = routines_doc(routines, changed_routines, &workspace)?;
            if find_named(doc.as_table_mut(), "bot", &to)?.is_some() {
                return Err(format!("bot {to:?} exists already"));
            }
            let t = find_named(doc.as_table_mut(), "bot", &from)?
                .ok_or_else(|| format!("there is no bot {from:?}"))?;
            t.insert("name", value(to.clone()));
            rename_refs(doc.as_table_mut(), &from, &to)?;
            effects.bot_renames.push((workspace, from, to));
            Ok(())
        }
        SettingsOp::BotMemoryRestore {
            workspace,
            name,
            version,
        } => {
            find_ws(workspaces, &workspace)?;
            let keep = crate::records::bots::KEEP_MEMORIES;
            if !(1..=keep).contains(&(version as usize)) {
                return Err(format!("version must be 1 to {keep}"));
            }
            effects.memory_restores.push((workspace, name, version));
            Ok(())
        }
        SettingsOp::AssistantUpdate {
            enabled,
            worker,
            soul_text,
        } => {
            let t = workspaces
                .as_table_mut()
                .entry("assistant")
                .or_insert_with(|| Item::Table(Table::new()))
                .as_table_like_mut()
                .ok_or("assistant is not a table; edit it by hand")?;
            set_bool(t, "enabled", enabled, true);
            set_str(t, "worker", worker.as_deref());
            if let Some(text) = soul_text {
                check_soul(&text)?;
                effects.assistant_soul = Some(text);
            }
            Ok(())
        }
        SettingsOp::BuildUpdate { dir, drop_after } => {
            let t = workspaces
                .as_table_mut()
                .entry("build")
                .or_insert_with(|| Item::Table(Table::new()))
                .as_table_like_mut()
                .ok_or("build is not a table; edit it by hand")?;
            if let Some(dir) = dir {
                set_str(t, "dir", dir.as_deref());
            }
            if let Some(after) = drop_after {
                set_str(t, "drop_after", after.as_deref());
            }
            drop_empty(workspaces.as_table_mut(), "build");
            Ok(())
        }
        SettingsOp::WorkerAdd { worker } => {
            let root = workspaces.as_table_mut();
            if find_named(root, "worker", &worker.name)?.is_some() {
                return Err(format!("worker {:?} exists already", worker.name));
            }
            let mut w = Table::new();
            w.insert("name", value(worker.name.clone()));
            write_worker(&mut w, &worker);
            aot(root, "worker")?.push(w);
            Ok(())
        }
        SettingsOp::WorkerUpdate { worker } => {
            let w = find_named(workspaces.as_table_mut(), "worker", &worker.name)?
                .ok_or_else(|| format!("there is no worker {:?}", worker.name))?;
            write_worker(w, &worker);
            Ok(())
        }
        SettingsOp::WorkerRemove { name } => {
            remove_named(workspaces.as_table_mut(), "worker", &name)
        }
    }
}

fn routines_doc<'a>(
    load: &mut dyn FnMut(&str) -> Result<DocumentMut, String>,
    changed: &'a mut BTreeMap<String, DocumentMut>,
    workspace: &str,
) -> Result<&'a mut DocumentMut, String> {
    if !changed.contains_key(workspace) {
        let doc = load(workspace)?;
        changed.insert(workspace.to_owned(), doc);
    }
    changed
        .get_mut(workspace)
        .ok_or_else(|| "the routines file was not loaded".to_owned())
}

fn keep_secret(
    effects: &mut Effects,
    workspace: &str,
    name: &str,
    secret: Option<WriteOnly>,
) -> Result<(), String> {
    if let Some(secret) = secret {
        secrets::check(secret.expose())?;
        effects.secrets.push((account(workspace, name), secret));
    }
    Ok(())
}

fn keep_instructions(
    effects: &mut Effects,
    workspace: &str,
    routine: &RoutineSpec,
    text: Option<String>,
) -> Result<(), String> {
    let Some(text) = text else { return Ok(()) };
    if text.len() > crate::records::routines::MAX_INSTRUCTIONS {
        return Err("instructions_text is larger than 12 KiB".into());
    }
    if text.contains('\0') {
        return Err("instructions_text has a NUL byte".into());
    }
    effects
        .instructions
        .push((workspace.to_owned(), routine.instructions.clone(), text));
    Ok(())
}

/// The Soul is written like an instruction file, at `bots/<name>/soul.md`.
fn keep_soul(
    effects: &mut Effects,
    workspace: &str,
    bot: &str,
    text: Option<String>,
) -> Result<(), String> {
    let Some(text) = text else { return Ok(()) };
    crate::records::bots::name(bot)?;
    check_soul(&text)?;
    let rel = format!(
        "{}/{bot}/{}",
        crate::records::bots::BOTS_DIR,
        crate::records::bots::SOUL_FILE
    );
    effects.instructions.push((workspace.to_owned(), rel, text));
    Ok(())
}

fn check_soul(text: &str) -> Result<(), String> {
    if text.len() > crate::records::bots::MAX_SOUL {
        return Err("soul_text is larger than 12 KiB".into());
    }
    if text.contains('\0') {
        return Err("soul_text has a NUL byte".into());
    }
    Ok(())
}

fn no_ws(name: &str) -> String {
    format!("no Workspace named {name:?}")
}

fn str_of<'a>(t: &'a Table, key: &str) -> Option<&'a str> {
    t.get(key).and_then(Item::as_str)
}

/// The `[[key]]` tables of `t`, made when missing. A list written another
/// way is left to the person who wrote it.
fn aot<'a>(t: &'a mut Table, key: &str) -> Result<&'a mut ArrayOfTables, String> {
    let item = t
        .entry(key)
        .or_insert_with(|| Item::ArrayOfTables(ArrayOfTables::new()));
    item.as_array_of_tables_mut()
        .ok_or_else(|| format!("{key} is not written as [[{key}]] tables; edit it by hand"))
}

fn find_ws<'a>(doc: &'a mut DocumentMut, name: &str) -> Result<&'a mut Table, String> {
    find_named(doc.as_table_mut(), "workspace", name)?.ok_or_else(|| no_ws(name))
}

fn find_named<'a>(
    t: &'a mut Table,
    key: &str,
    name: &str,
) -> Result<Option<&'a mut Table>, String> {
    if !t.contains_key(key) {
        return Ok(None);
    }
    Ok(aot(t, key)?
        .iter_mut()
        .find(|e| str_of(e, "name") == Some(name)))
}

fn remove_named(t: &mut Table, key: &str, name: &str) -> Result<(), String> {
    let missing = || format!("there is no {key} {name:?}");
    if !t.contains_key(key) {
        return Err(missing());
    }
    let list = aot(t, key)?;
    let i = (0..list.len())
        .find(|&i| list.get(i).is_some_and(|e| str_of(e, "name") == Some(name)))
        .ok_or_else(missing)?;
    list.remove(i);
    if list.is_empty() {
        t.remove(key);
    }
    Ok(())
}

fn names_in(t: &Table, key: &str) -> Vec<String> {
    t.get(key)
        .and_then(Item::as_array_of_tables)
        .map(|list| {
            list.iter()
                .filter_map(|e| str_of(e, "name").map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// The table at `key`, made as a `[...]` table when missing.
fn sub_table<'a>(t: &'a mut dyn TableLike, key: &str) -> Result<&'a mut dyn TableLike, String> {
    if t.get(key).is_none() {
        t.insert(key, Item::Table(Table::new()));
    }
    t.get_mut(key)
        .and_then(Item::as_table_like_mut)
        .ok_or_else(|| format!("{key} is not a table; edit it by hand"))
}

fn drop_empty(t: &mut dyn TableLike, key: &str) {
    if t.get(key)
        .and_then(Item::as_table_like)
        .is_some_and(TableLike::is_empty)
    {
        t.remove(key);
    }
}

/// Sets a string, keeping the old one's formatting when it is equal; None removes.
fn set_str(t: &mut dyn TableLike, key: &str, v: Option<&str>) {
    match v {
        Some(v) if t.get(key).and_then(Item::as_str) == Some(v) => {}
        Some(v) => {
            t.insert(key, value(v));
        }
        None => {
            t.remove(key);
        }
    }
}

/// Removes the key when `v` is the default.
fn set_bool(t: &mut dyn TableLike, key: &str, v: bool, keep_default: bool) {
    if !v && !keep_default {
        t.remove(key);
    } else if t.get(key).and_then(Item::as_bool) != Some(v) {
        t.insert(key, value(v));
    }
}

fn same_list(item: Option<&Item>, list: &[String]) -> bool {
    item.and_then(Item::as_array).is_some_and(|a| {
        a.len() == list.len() && a.iter().zip(list).all(|(x, y)| x.as_str() == Some(y))
    })
}

/// An empty list removes the key.
fn set_list(t: &mut dyn TableLike, key: &str, list: &[String]) {
    if list.is_empty() {
        t.remove(key);
    } else if !same_list(t.get(key), list) {
        t.insert(key, value(Array::from_iter(list)));
    }
}

/// Keeps an empty list: for a worker, no Workspace differs from every one.
fn set_opt_list(t: &mut dyn TableLike, key: &str, list: Option<&Vec<String>>) {
    match list {
        None => {
            t.remove(key);
        }
        Some(list) if !same_list(t.get(key), list) => {
            t.insert(key, value(Array::from_iter(list)));
        }
        Some(_) => {}
    }
}

fn set_map(t: &mut dyn TableLike, key: &str, map: &BTreeMap<String, String>) {
    if map.is_empty() {
        t.remove(key);
        return;
    }
    let same = t.get(key).and_then(Item::as_table_like).is_some_and(|old| {
        old.len() == map.len()
            && map
                .iter()
                .all(|(k, v)| old.get(k).and_then(Item::as_str) == Some(v))
    });
    if !same {
        let inline: InlineTable = map.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        t.insert(key, value(inline));
    }
}

fn write_connection(t: &mut Table, c: &ConnectionSpec) {
    set_str(t, "env", Some(&c.env));
    set_str(t, "account", c.account.as_deref());
    let access = c.access.map(|a| match a {
        Access::Read => "read",
        Access::Write => "write",
    });
    set_str(t, "access", access);
    // Ignored since ADR 0075: an edit drops it.
    t.remove("default");
}

fn write_mcp(t: &mut Table, m: &McpSpec) {
    set_str(t, "command", m.command.as_deref());
    match &m.args {
        Some(args) if !args.is_empty() => set_list(t, "args", args),
        _ => {
            t.remove("args");
        }
    }
    set_str(t, "url", m.url.as_deref());
    set_str(t, "type", m.kind.as_deref());
    set_map(t, "env", &m.env);
    set_map(t, "headers", &m.headers);
    set_list(t, "connections", &m.connections);
    t.remove("default");
}

fn write_routine(t: &mut Table, r: &RoutineSpec) -> Result<(), String> {
    set_str(t, "at", r.at.as_deref());
    set_str(t, "schedule", r.schedule.as_deref());
    set_str(t, "timezone", r.timezone.as_deref());
    set_str(t, "worker", r.worker.as_deref());
    set_str(t, "instructions", Some(&r.instructions));
    set_str(t, "project", r.project.as_deref());
    set_str(t, "bot", r.bot.as_deref());
    set_bool(t, "read_only", r.read_only, false);
    set_str(t, "on", r.on.as_deref());
    // Ignored since ADR 0075: an edit drops them.
    t.remove("connections");
    t.remove("mcp");
    set_bool(t, "enabled", r.enabled, true);
    if r.items.is_none() && !r.direct_writes {
        t.remove("output");
        return Ok(());
    }
    let out = sub_table(t, "output")?;
    match r.items {
        Some(n) if out.get("items").and_then(Item::as_integer) != Some(i64::from(n)) => {
            out.insert("items", value(i64::from(n)));
        }
        Some(_) => {}
        None => {
            out.remove("items");
        }
    }
    set_bool(out, "direct_writes", r.direct_writes, false);
    Ok(())
}

/// Every Routine with `bot = from` and every team that lists `from`, now `to`.
fn rename_refs(root: &mut Table, from: &str, to: &str) -> Result<(), String> {
    for r in aot(root, "routine")?.iter_mut() {
        if r.get("bot").and_then(Item::as_str) == Some(from) {
            r.insert("bot", value(to));
        }
    }
    for b in aot(root, "bot")?.iter_mut() {
        let Some(members) = b.get("members").and_then(Item::as_array) else {
            continue;
        };
        if members.iter().any(|m| m.as_str() == Some(from)) {
            let renamed: Vec<String> = members
                .iter()
                .filter_map(|m| m.as_str())
                .map(|m| {
                    if m == from {
                        to.to_owned()
                    } else {
                        m.to_owned()
                    }
                })
                .collect();
            set_list(b, "members", &renamed);
        }
    }
    Ok(())
}

fn write_bot(t: &mut Table, b: &BotSpec) {
    set_str(t, "description", b.description.as_deref());
    set_str(t, "worker", b.worker.as_deref());
    // ADR 0094: no daily limit. An older file's key goes on the next edit.
    t.remove("daily_items");
    set_bool(t, "enabled", b.enabled, true);
    set_list(t, "members", &b.members);
    match b.handoffs_per_day {
        Some(n) if t.get("handoffs_per_day").and_then(Item::as_integer) != Some(i64::from(n)) => {
            t.insert("handoffs_per_day", value(i64::from(n)));
        }
        Some(_) => {}
        None => {
            t.remove("handoffs_per_day");
        }
    }
    if b.commands.is_empty() {
        t.remove("commands");
    } else {
        set_list(t, "commands", &b.commands);
    }
}

fn write_worker(t: &mut Table, w: &WorkerSpec) {
    set_str(t, "description", w.description.as_deref());
    set_str(t, "agent", Some(&w.agent));
    set_list(t, "args", &w.args);
    set_str(t, "instructions", w.instructions.as_deref());
    set_opt_list(t, "workspaces", w.workspaces.as_ref());
    match &w.task_loop {
        None => {
            t.remove("loop");
        }
        Some(lp) => {
            let mut table = Table::new();
            table.insert("check", value(lp.check.clone()));
            if let Some(n) = lp.max_attempts {
                table.insert("max_attempts", value(i64::from(n)));
            }
            if let Some(n) = lp.timeout {
                table.insert("timeout", value(i64::try_from(n).unwrap_or(i64::MAX)));
            }
            t.insert("loop", Item::Table(table));
        }
    }
}

/// The document as plain TOML values, to check that writing it out and
/// reading it back means the same: a table printed under the wrong
/// `[[workspace]]` would not.
pub fn plain(doc: &DocumentMut) -> toml::Table {
    table_like(doc.as_table())
}

fn table_like(t: &dyn TableLike) -> toml::Table {
    t.iter()
        .filter_map(|(k, item)| Some((k.to_owned(), item_value(item)?)))
        .collect()
}

fn item_value(item: &Item) -> Option<toml::Value> {
    Some(match item {
        Item::None => return None,
        Item::Value(v) => edit_value(v),
        Item::Table(t) => toml::Value::Table(table_like(t)),
        Item::ArrayOfTables(a) => toml::Value::Array(
            a.iter()
                .map(|t| toml::Value::Table(table_like(t)))
                .collect(),
        ),
    })
}

fn edit_value(v: &toml_edit::Value) -> toml::Value {
    use toml_edit::Value as V;
    match v {
        V::String(s) => toml::Value::String(s.value().clone()),
        V::Integer(i) => toml::Value::Integer(*i.value()),
        V::Float(f) => toml::Value::Float(*f.value()),
        V::Boolean(b) => toml::Value::Boolean(*b.value()),
        V::Datetime(d) => d
            .value()
            .to_string()
            .parse()
            .map(toml::Value::Datetime)
            .unwrap_or_else(|_| toml::Value::String(d.value().to_string())),
        V::Array(a) => toml::Value::Array(a.iter().map(edit_value).collect()),
        V::InlineTable(t) => toml::Value::Table(table_like(t)),
    }
}
