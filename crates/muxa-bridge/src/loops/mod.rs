//! The Task Loop engine (ADR 0019 section 3): after each finished turn of a
//! Task's agent, run its check; on a fail, type a fixed prompt; on a pass,
//! hand the Task to the user. `step` decides, `check` runs the command, and
//! `typing` types only when no person does.

// Until the daemon's tick calls it (ADR 0019 P2, after P1b and P1c).
#![allow(dead_code)]

pub mod check;
pub mod step;
pub mod typing;
