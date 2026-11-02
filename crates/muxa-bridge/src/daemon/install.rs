//! The service file that starts the daemon at login: a LaunchAgent on macOS,
//! a systemd user unit on Linux. Every `launchctl` and `systemctl` call goes
//! through [`Runner`], so tests never touch the real service manager.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

pub const LABEL: &str = "tech.xdge.muxa.daemon";
pub const UNIT: &str = "muxa-daemon.service";
/// launchd's stderr: only start-up failures land here; the daemon logs to `daemon.log`.
pub const ERR_FILE: &str = "daemon.err";
/// The environment `install` copies from its shell, and no other.
pub const COPIED_ENV: [&str; 3] = ["HOME", "TMUX_TMPDIR", "PATH"];

/// What the service file starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spec {
    pub binary: PathBuf,
    pub tmux: PathBuf,
    /// Serve-mode flags, as `Args::service_flags` gives them.
    pub flags: Vec<String>,
    pub daemon_dir: PathBuf,
    pub env: Vec<(String, String)>,
}

impl Spec {
    pub fn arguments(&self) -> Vec<String> {
        let mut out = vec![
            self.binary.display().to_string(),
            "daemon".into(),
            "run".into(),
            "--tmux".into(),
            self.tmux.display().to_string(),
        ];
        out.extend(self.flags.iter().cloned());
        out.push("--daemon-dir".into());
        out.push(self.daemon_dir.display().to_string());
        out
    }
}

/// The service manager of this host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Platform {
    Launchd { uid: u32 },
    Systemd,
}

impl Platform {
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Launchd {
                uid: rustix::process::getuid().as_raw(),
            }
        } else {
            Self::Systemd
        }
    }

    pub fn file(&self, home: &Path) -> PathBuf {
        match self {
            Self::Launchd { .. } => home.join(format!("Library/LaunchAgents/{LABEL}.plist")),
            Self::Systemd => home.join(format!(".config/systemd/user/{UNIT}")),
        }
    }

    pub fn render(&self, spec: &Spec) -> String {
        match self {
            Self::Launchd { .. } => plist(spec),
            Self::Systemd => unit(spec),
        }
    }

    fn target(&self) -> String {
        match self {
            Self::Launchd { uid } => format!("gui/{uid}/{LABEL}"),
            Self::Systemd => UNIT.into(),
        }
    }

    /// Whether the service is loaded; for launchd, in the GUI domain.
    pub fn loaded(&self, run: &dyn Runner) -> bool {
        match self {
            Self::Launchd { .. } => run.run("launchctl", &["print", &self.target()]).is_ok(),
            Self::Systemd => run
                .run("systemctl", &["--user", "is-active", "--quiet", UNIT])
                .is_ok(),
        }
    }

    /// Loads the file and starts the daemon; replaces a loaded older one.
    pub fn load(&self, run: &dyn Runner, file: &Path) -> Result<(), String> {
        let file = file.display().to_string();
        match self {
            Self::Launchd { uid } => {
                if self.loaded(run) {
                    run.run("launchctl", &["bootout", &self.target()])?;
                }
                run.run("launchctl", &["bootstrap", &format!("gui/{uid}"), &file])
            }
            Self::Systemd => {
                run.run("systemctl", &["--user", "daemon-reload"])?;
                run.run("systemctl", &["--user", "enable", UNIT])?;
                run.run("systemctl", &["--user", "restart", UNIT])
            }
        }
    }

    /// Stops the daemon. The file stays, so it starts again at the next login.
    pub fn unload(&self, run: &dyn Runner) -> Result<(), String> {
        match self {
            Self::Launchd { .. } if self.loaded(run) => {
                run.run("launchctl", &["bootout", &self.target()])
            }
            Self::Launchd { .. } => Ok(()),
            Self::Systemd => run.run("systemctl", &["--user", "stop", UNIT]),
        }
    }

    /// Restarts the daemon, for example after an upgrade. Loops go on.
    pub fn restart(&self, run: &dyn Runner, file: &Path) -> Result<(), String> {
        match self {
            Self::Launchd { .. } if self.loaded(run) => {
                run.run("launchctl", &["kickstart", "-k", &self.target()])
            }
            Self::Launchd { .. } => self.load(run, file),
            Self::Systemd => run.run("systemctl", &["--user", "restart", UNIT]),
        }
    }

    /// Unloads the service so that it does not start at the next login.
    pub fn disable(&self, run: &dyn Runner) -> Result<(), String> {
        match self {
            Self::Launchd { .. } => self.unload(run),
            Self::Systemd => run.run("systemctl", &["--user", "disable", "--now", UNIT]),
        }
    }

    /// What the user should know after `install`.
    pub fn hint(&self) -> Option<&'static str> {
        match self {
            Self::Launchd { .. } => None,
            Self::Systemd => Some(
                "A user service stops at logout. To keep the daemon running, run: loginctl enable-linger",
            ),
        }
    }
}

/// Runs `launchctl` or `systemctl`. A failure carries the tool's stderr.
pub trait Runner {
    fn run(&self, program: &str, args: &[&str]) -> Result<(), String>;
}

/// The real service manager.
pub struct System;

impl Runner for System {
    fn run(&self, program: &str, args: &[&str]) -> Result<(), String> {
        let out = std::process::Command::new(program)
            .args(args)
            .output()
            .map_err(|e| format!("cannot run {program}: {e}"))?;
        if out.status.success() {
            Ok(())
        } else {
            Err(format!(
                "{program} {} failed: {}",
                args.first().unwrap_or(&""),
                String::from_utf8_lossy(&out.stderr).trim()
            ))
        }
    }
}

fn xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// A LaunchAgent: started at login, restarted after a crash but not after a
/// clean exit, which is how `daemon stop` ends it.
pub fn plist(spec: &Spec) -> String {
    let mut s = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n<dict>\n",
    );
    let _ = writeln!(s, "\t<key>Label</key>\n\t<string>{LABEL}</string>");
    s.push_str("\t<key>ProgramArguments</key>\n\t<array>\n");
    for a in spec.arguments() {
        let _ = writeln!(s, "\t\t<string>{}</string>", xml(&a));
    }
    s.push_str("\t</array>\n\t<key>EnvironmentVariables</key>\n\t<dict>\n");
    for (k, v) in &spec.env {
        let _ = writeln!(
            s,
            "\t\t<key>{}</key>\n\t\t<string>{}</string>",
            xml(k),
            xml(v)
        );
    }
    s.push_str(
        "\t</dict>\n\
         \t<key>RunAtLoad</key>\n\t<true/>\n\
         \t<key>KeepAlive</key>\n\t<dict>\n\t\t<key>SuccessfulExit</key>\n\t\t<false/>\n\t</dict>\n\
         \t<key>ThrottleInterval</key>\n\t<integer>10</integer>\n",
    );
    let err = spec.daemon_dir.join(ERR_FILE);
    let _ = writeln!(
        s,
        "\t<key>StandardErrorPath</key>\n\t<string>{}</string>",
        xml(&err.display().to_string())
    );
    s.push_str("</dict>\n</plist>\n");
    s
}

/// One systemd word: quoted, with `%` specifiers and `$` expansion escaped.
fn systemd_word(s: &str) -> String {
    let escaped = s
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('%', "%%")
        .replace('$', "$$");
    format!("\"{escaped}\"")
}

/// A systemd user unit, restarted after a crash but not after a clean exit.
pub fn unit(spec: &Spec) -> String {
    let exec: Vec<String> = spec.arguments().iter().map(|a| systemd_word(a)).collect();
    let mut s = String::from("[Unit]\nDescription=muxa daemon (the Task Loop)\n\n[Service]\n");
    let _ = writeln!(s, "ExecStart={}", exec.join(" "));
    for (k, v) in &spec.env {
        // `$` is not expanded in Environment=, but `%` specifiers are.
        let value = format!("{k}={v}")
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('%', "%%");
        let _ = writeln!(s, "Environment=\"{value}\"");
    }
    s.push_str("Restart=on-failure\nRestartSec=5\n\n[Install]\nWantedBy=default.target\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn spec() -> Spec {
        Spec {
            binary: "/opt/muxa/bin/muxa-bridge".into(),
            tmux: "/opt/homebrew/bin/tmux".into(),
            flags: vec![
                "--socket".into(),
                "work".into(),
                "--workspaces".into(),
                "/Users/example/muxa & co/workspaces.toml".into(),
                "--task-command".into(),
                "claude --model \"$M\" 100%".into(),
            ],
            daemon_dir: "/Users/example/.local/state/muxa".into(),
            env: vec![
                ("HOME".into(), "/Users/example".into()),
                ("TMUX_TMPDIR".into(), "/tmp/example".into()),
                ("PATH".into(), "/opt/homebrew/bin:/usr/bin:/bin".into()),
            ],
        }
    }

    #[test]
    fn the_launch_agent_matches_its_golden_file() {
        assert_eq!(plist(&spec()), include_str!("golden/launch-agent.plist"));
    }

    #[test]
    fn the_systemd_unit_matches_its_golden_file() {
        assert_eq!(unit(&spec()), include_str!("golden/muxa-daemon.service"));
    }

    /// Records each call; `loaded` answers the `print` and `is-active` checks.
    struct Fake {
        calls: RefCell<Vec<String>>,
        loaded: bool,
    }

    impl Fake {
        fn new(loaded: bool) -> Self {
            Self {
                calls: RefCell::new(Vec::new()),
                loaded,
            }
        }

        fn calls(&self) -> Vec<String> {
            self.calls.borrow().clone()
        }
    }

    impl Runner for Fake {
        fn run(&self, program: &str, args: &[&str]) -> Result<(), String> {
            self.calls
                .borrow_mut()
                .push(format!("{program} {}", args.join(" ")));
            let check = args.first() == Some(&"print") || args.contains(&"is-active");
            if check && !self.loaded {
                return Err("not loaded".into());
            }
            Ok(())
        }
    }

    #[test]
    fn launchd_replaces_a_loaded_agent_and_kickstarts_on_restart() {
        let p = Platform::Launchd { uid: 501 };
        let file = p.file(Path::new("/Users/example"));
        assert_eq!(
            file,
            Path::new("/Users/example/Library/LaunchAgents/tech.xdge.muxa.daemon.plist")
        );

        let run = Fake::new(true);
        p.load(&run, &file).unwrap();
        p.restart(&run, &file).unwrap();
        p.unload(&run).unwrap();
        assert_eq!(
            run.calls(),
            [
                "launchctl print gui/501/tech.xdge.muxa.daemon",
                "launchctl bootout gui/501/tech.xdge.muxa.daemon",
                "launchctl bootstrap gui/501 /Users/example/Library/LaunchAgents/tech.xdge.muxa.daemon.plist",
                "launchctl print gui/501/tech.xdge.muxa.daemon",
                "launchctl kickstart -k gui/501/tech.xdge.muxa.daemon",
                "launchctl print gui/501/tech.xdge.muxa.daemon",
                "launchctl bootout gui/501/tech.xdge.muxa.daemon",
            ]
        );

        let run = Fake::new(false);
        p.restart(&run, &file).unwrap();
        p.unload(&run).unwrap();
        assert_eq!(
            run.calls(),
            [
                "launchctl print gui/501/tech.xdge.muxa.daemon",
                "launchctl print gui/501/tech.xdge.muxa.daemon",
                "launchctl bootstrap gui/501 /Users/example/Library/LaunchAgents/tech.xdge.muxa.daemon.plist",
                "launchctl print gui/501/tech.xdge.muxa.daemon",
            ],
            "a restart of an unloaded agent loads it; an unload does nothing"
        );
    }

    #[test]
    fn systemd_enables_restarts_and_disables_the_user_unit() {
        let p = Platform::Systemd;
        let file = p.file(Path::new("/home/example"));
        assert_eq!(
            file,
            Path::new("/home/example/.config/systemd/user/muxa-daemon.service")
        );
        let run = Fake::new(true);
        p.load(&run, &file).unwrap();
        p.unload(&run).unwrap();
        p.disable(&run).unwrap();
        assert_eq!(
            run.calls(),
            [
                "systemctl --user daemon-reload",
                "systemctl --user enable muxa-daemon.service",
                "systemctl --user restart muxa-daemon.service",
                "systemctl --user stop muxa-daemon.service",
                "systemctl --user disable --now muxa-daemon.service",
            ]
        );
        assert!(p.hint().unwrap().contains("loginctl enable-linger"));
    }
}
