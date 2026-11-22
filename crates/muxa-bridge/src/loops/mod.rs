//! A Task Loop's check (ADR 0019): `check` runs it and writes its report;
//! the loop itself is `v2::task_loop`.

pub mod check;

/// How a check ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckEnd {
    Exited { code: i32, seconds: u64 },
    TimedOut { seconds: u64 },
}

/// The fail prompt: fixed text, only the attempt changes (ADR 0019).
pub fn fail_prompt(attempt: u32) -> String {
    format!(
        "The check failed. Its output is in .muxa/checks/{attempt}.log. \
         Fix the cause, then end your turn."
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_fail_prompt_is_fixed() {
        assert_eq!(
            super::fail_prompt(2),
            "The check failed. Its output is in .muxa/checks/2.log. Fix the cause, then end your turn."
        );
    }
}
