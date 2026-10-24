//! `muxa-bridge`: serves the muxa protocol on stdin/stdout, started over SSH.

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--version") {
        println!("muxa-bridge {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    eprintln!("muxa-bridge: not implemented yet (see docs/plan.md, phase 3)");
    std::process::exit(1);
}
