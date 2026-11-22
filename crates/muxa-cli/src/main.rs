//! `muxa-cli`: the reference client of protocol v2 (`protocol/PROTOCOL-v2.md`).
//! `muxa-cli v2 …` is the same: scripts and agents still call it that way.

mod raw;
mod v2;

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|a| a == "v2") {
        args.remove(0);
    }
    match v2::main(args) {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("muxa-cli: {e}");
            ExitCode::FAILURE
        }
    }
}
