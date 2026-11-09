use std::path::Path;

use crate::records::Workspaces;

fn parse(extra: &str, ws_extra: &str) -> Result<Workspaces, String> {
    let root = std::env::temp_dir();
    let root = root.to_string_lossy();
    let text = format!(
        "{extra}\n[[workspace]]\nname = \"w\"\nroot = \"{root}\"\n{ws_extra}\n\
         [[worker]]\nname = \"Elsewhere\"\nagent = \"claude-code\"\nworkspaces = [\"w2\"]\n\
         [[worker]]\nname = \"Researcher\"\nagent = \"claude-code\"\n\
         [[workspace]]\nname = \"w2\"\nroot = \"{root}\"\n"
    );
    Workspaces::parse(&text, Path::new("w.toml"), None).map_err(|e| e.to_string())
}

#[test]
fn without_tables_the_host_is_off_and_no_workspace_delegates() {
    let w = parse("", "").unwrap();
    assert!(!w.assistant().enabled);
    assert_eq!(w.assistant().max_runs_per_day, 20);
    assert_eq!(w.all()[0].assistant, None);
    assert_eq!(w.delegate("w"), None);
}

#[test]
fn a_delegate_gets_the_defaults() {
    let w = parse(
        "[assistant]\nenabled = true",
        "[workspace.assistant]\ndelegate = true",
    )
    .unwrap();
    let d = w.delegate("w").unwrap();
    assert_eq!(
        (d.worker.as_deref(), d.max_open_runs, d.max_runs_per_day),
        (None, 1, 10)
    );
    assert_eq!(d.run_timeout, 1800);
    assert_eq!(d.notes, super::NotesMode::Review);
    assert_eq!(w.delegate("w2"), None);
}

#[test]
fn values_are_kept_and_the_host_switch_gates_delegates() {
    let ws = "[workspace.assistant]\ndelegate = true\nworker = \"Researcher\"\n\
              max_open_runs = 2\nmax_runs_per_day = 4\nrun_timeout = \"2h\"\nnotes = \"auto\"";
    let w = parse("[assistant]\nenabled = true\nmax_runs_per_day = 7", ws).unwrap();
    assert_eq!(w.assistant().max_runs_per_day, 7);
    let d = w.delegate("w").unwrap();
    assert_eq!(d.worker.as_deref(), Some("Researcher"));
    assert_eq!(
        (d.max_open_runs, d.max_runs_per_day, d.run_timeout),
        (2, 4, 7200)
    );
    assert_eq!(d.notes, super::NotesMode::Auto);
    let off = parse("[assistant]\nenabled = false", ws).unwrap();
    assert!(off.all()[0].assistant.is_some());
    assert_eq!(off.delegate("w"), None);
    let no = parse(
        "[assistant]\nenabled = true",
        "[workspace.assistant]\ndelegate = false",
    )
    .unwrap();
    assert_eq!(no.delegate("w"), None);
}

#[test]
fn counterexamples_are_refused_by_field() {
    let on = "[assistant]\nenabled = true";
    let cases = [
        ("[assistant]\nenabled = true\nlimit = 3", "", "limit"),
        ("[assistant]\nmax_runs_per_day = 3", "", "enabled"),
        (
            "[assistant]\nenabled = true\nmax_runs_per_day = 0",
            "",
            "max_runs_per_day",
        ),
        (on, "[workspace.assistant]\nmax_open_runs = 1", "delegate"),
        (
            on,
            "[workspace.assistant]\ndelegate = true\nworkers = \"x\"",
            "workers",
        ),
        (
            on,
            "[workspace.assistant]\ndelegate = true\nmax_open_runs = 0",
            "max_open_runs",
        ),
        (
            on,
            "[workspace.assistant]\ndelegate = true\nmax_open_runs = -1",
            "max_open_runs",
        ),
        (
            on,
            "[workspace.assistant]\ndelegate = true\nmax_runs_per_day = 0",
            "max_runs_per_day",
        ),
        (
            on,
            "[workspace.assistant]\ndelegate = true\nrun_timeout = \"soon\"",
            "run_timeout",
        ),
        (
            on,
            "[workspace.assistant]\ndelegate = true\nrun_timeout = \"0m\"",
            "run_timeout",
        ),
        (
            on,
            "[workspace.assistant]\ndelegate = false\nrun_timeout = \"30\"",
            "run_timeout",
        ),
        (
            on,
            "[workspace.assistant]\ndelegate = true\nnotes = \"always\"",
            "notes must be",
        ),
        (
            on,
            "[workspace.assistant]\ndelegate = true\nworker = \"Nobody\"",
            "not configured",
        ),
        (
            on,
            "[workspace.assistant]\ndelegate = true\nworker = \"Elsewhere\"",
            "not allowed",
        ),
    ];
    for (host, ws, needle) in cases {
        let err = parse(host, ws).unwrap_err();
        assert!(err.contains(needle), "{host:?} {ws:?}: {err}");
    }
}

#[test]
fn the_default_worker_name_means_the_workspace_agent() {
    let w = parse(
        "[assistant]\nenabled = true",
        &format!(
            "[workspace.assistant]\ndelegate = true\nworker = \"{}\"",
            muxa_protocol::DEFAULT_WORKER
        ),
    )
    .unwrap();
    assert_eq!(w.delegate("w").unwrap().worker, None);
}

/// `w` with `ws_extra`, `w2` with `w2_extra`, and the host on.
fn two(ws_extra: &str, w2_extra: &str) -> Result<Workspaces, String> {
    let root = std::env::temp_dir();
    let root = root.to_string_lossy();
    let text = format!(
        "[assistant]\nenabled = true\n\
         [[workspace]]\nname = \"w\"\nroot = \"{root}\"\n{ws_extra}\n\
         [[workspace]]\nname = \"w2\"\nroot = \"{root}\"\n{w2_extra}\n"
    );
    Workspaces::parse(&text, Path::new("w.toml"), None).map_err(|e| e.to_string())
}

#[test]
fn share_and_overall_requests_follow_delegate_by_default() {
    use muxa_protocol::{OverallRequests, Share};
    let w = two(
        "[workspace.assistant]\ndelegate = true",
        "[workspace.assistant]\ndelegate = false",
    )
    .unwrap();
    assert_eq!(w.share("w"), Share::Summaries);
    assert_eq!(w.overall_requests("w"), OverallRequests::Review);
    assert_eq!(w.share("w2"), Share::None);
    // It takes no requests, so the overall assistant may not ask it.
    assert_eq!(w.overall_requests("w2"), OverallRequests::None);
    assert_eq!(w.share("nobody"), Share::None);
    assert_eq!(w.overall(), None);
    assert_eq!(w.assistant().max_auto_requests_per_day, 5);

    let set = two(
        "[workspace.assistant]\ndelegate = true\nshare = \"outline\"\noverall_requests = \"auto\"",
        "[workspace.assistant]\ndelegate = false\nshare = \"summaries\"",
    )
    .unwrap();
    assert_eq!(set.share("w"), Share::Outline);
    assert_eq!(set.overall_requests("w"), OverallRequests::Auto);
    assert_eq!(set.share("w2"), Share::Summaries);
    let none = two(
        "[workspace.assistant]\ndelegate = true\nshare = \"none\"\noverall_requests = \"none\"",
        "",
    )
    .unwrap();
    assert_eq!(none.share("w"), Share::None);
    assert_eq!(none.overall_requests("w"), OverallRequests::None);
    assert_eq!(none.share("w2"), Share::None);
}

#[test]
fn the_overall_workspace_gets_its_own_defaults_and_takes_no_requests() {
    use muxa_protocol::Share;
    let w = two(
        "[workspace.assistant]\noverall = true",
        "[workspace.assistant]\ndelegate = true",
    )
    .unwrap();
    let overall = w.overall().unwrap();
    assert_eq!(overall.name, "w");
    let runs = overall.assistant.as_ref().unwrap();
    assert_eq!(
        (runs.max_open_runs, runs.max_runs_per_day, runs.run_timeout),
        (1, 8, 900)
    );
    assert_eq!(w.delegate("w"), None);
    assert_eq!(w.share("w"), Share::None);
    assert!(w.delegate("w2").is_some());

    let set = two(
        "[workspace.assistant]\noverall = true\ndelegate = false\nmax_open_runs = 2\n\
         max_runs_per_day = 3\nrun_timeout = \"1h\"\nnotes = \"auto\"",
        "",
    )
    .unwrap();
    let runs = set.overall().unwrap().assistant.as_ref().unwrap();
    assert_eq!(
        (runs.max_open_runs, runs.max_runs_per_day, runs.run_timeout),
        (2, 3, 3600)
    );
    assert_eq!(runs.notes, super::NotesMode::Auto);

    let off = Workspaces::parse(
        &format!(
            "[[workspace]]\nname = \"w\"\nroot = \"{}\"\n[workspace.assistant]\noverall = true\n",
            std::env::temp_dir().to_string_lossy()
        ),
        Path::new("w.toml"),
        None,
    )
    .unwrap();
    assert_eq!(off.overall(), None);
}

#[test]
fn the_overall_workspace_plans_weekday_mornings_unless_told_otherwise() {
    let w = two(
        "[workspace.assistant]\noverall = true",
        "[workspace.assistant]\ndelegate = true",
    )
    .unwrap();
    let plans = &w.overall().unwrap().assistant.as_ref().unwrap().plans;
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].schedule, super::DEFAULT_PLAN);
    assert_eq!(plans[0].tz, jiff::tz::TimeZone::system());
    assert!(w.delegate("w2").unwrap().plans.is_empty());

    let set = two(
        "[workspace.assistant]\noverall = true\nplans = [\"30 7 * * 1-5\", \"0 13 * * *\"]\n\
         timezone = \"Asia/Tokyo\"",
        "",
    )
    .unwrap();
    let plans = &set.overall().unwrap().assistant.as_ref().unwrap().plans;
    let shown: Vec<String> = plans.iter().map(|t| format!("{t:?}")).collect();
    assert_eq!(shown, ["30 7 * * 1-5 Asia/Tokyo", "0 13 * * * Asia/Tokyo"]);

    let none = two("[workspace.assistant]\noverall = true\nplans = []", "").unwrap();
    assert!(
        none.overall()
            .unwrap()
            .assistant
            .as_ref()
            .unwrap()
            .plans
            .is_empty(),
        "an empty list plans on demand only"
    );
}

#[test]
fn overall_counterexamples_are_refused_by_field() {
    let cases = [
        (
            "[workspace.assistant]\noverall = true",
            "[workspace.assistant]\noverall = true",
            "workspaces \"w\" and \"w2\" both set",
        ),
        (
            "[workspace.assistant]\ndelegate = true\nshare = \"all\"",
            "",
            "share must be",
        ),
        (
            "[workspace.assistant]\ndelegate = true\nshare = \"Summaries\"",
            "",
            "share must be",
        ),
        (
            "[workspace.assistant]\ndelegate = true\noverall_requests = \"always\"",
            "",
            "overall_requests must be",
        ),
        (
            "[workspace.assistant]\noverall = true\ndelegate = true",
            "",
            "takes no requests",
        ),
        (
            "[workspace.assistant]\noverall = true\nshare = \"outline\"",
            "",
            "do not apply",
        ),
        (
            "[workspace.assistant]\noverall = true\noverall_requests = \"auto\"",
            "",
            "do not apply",
        ),
        (
            "[workspace.assistant]\noverall = \"yes\"",
            "",
            "expected a boolean",
        ),
        (
            "[workspace.assistant]\noverall = true\nrun_timeout = \"soon\"",
            "",
            "run_timeout",
        ),
        (
            "[workspace.assistant]\noverall = true\nplans = [\"30 7 * * 1-5\", \"0 12 * * *\", \
             \"0 17 * * *\", \"0 20 * * *\"]",
            "",
            "more than 3 schedules",
        ),
        (
            "[workspace.assistant]\noverall = true\nplans = [\"0 9 * * *\", \"0 25 * * *\"]",
            "",
            "plans[1]: schedule hour",
        ),
        (
            "[workspace.assistant]\noverall = true\nplans = [\"0 9 * * *\", \"0  9 * * *\"]",
            "",
            "plans[1] appears twice",
        ),
        (
            "[workspace.assistant]\noverall = true\ntimezone = \"Mars/Base\"",
            "",
            "not a known IANA time zone",
        ),
        (
            "[workspace.assistant]\ndelegate = true\nplans = [\"0 9 * * *\"]",
            "",
            "only to the overall workspace",
        ),
        (
            "[workspace.assistant]\ndelegate = true\ntimezone = \"UTC\"",
            "",
            "only to the overall workspace",
        ),
    ];
    for (ws, w2, needle) in cases {
        let err = two(ws, w2).unwrap_err();
        assert!(err.contains(needle), "{ws:?} {w2:?}: {err}");
    }
    let err = parse(
        "[assistant]\nenabled = true\nmax_auto_requests_per_day = 0",
        "",
    )
    .unwrap_err();
    assert!(err.contains("max_auto_requests_per_day"), "{err}");
}
