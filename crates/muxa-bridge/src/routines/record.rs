//! A Routine run's output check limits and its fixed fail prompt (ADR 0026
//! section 3).

pub const MAX_ATTEMPTS: u32 = 3;

/// The fixed prompt after a failed output check (ADR 0026 section 3).
pub fn fail_prompt(attempt: u32) -> String {
    format!(
        "The Routine output did not pass the check. The reasons are in .muxa/checks/{attempt}.log. \
         Fix .muxa/routine/output.json, then end your turn."
    )
}
