//! `muxa-cli`: a terminal client for the muxa protocol, the reference client.

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--version") {
        println!("muxa-cli {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    eprintln!("muxa-cli: not implemented yet (see docs/plan.md, phase 5)");
    std::process::exit(1);
}
