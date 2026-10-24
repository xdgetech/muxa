//! tmux object ids. muxa targets panes, windows and sessions only by id:
//! name targets resolve ambiguously (`docs/findings/tmux-control-mode.md`).

use std::fmt;
use std::str::FromStr;

use crate::Error;

macro_rules! tmux_id {
    ($name:ident, $sigil:literal, $what:literal) => {
        #[doc = concat!("A tmux ", $what, " id such as `", $sigil, "3`.")]
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(u32);

        impl $name {
            pub fn new(n: u32) -> Self {
                Self(n)
            }

            pub fn number(self) -> u32 {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}{}", $sigil, self.0)
            }
        }

        impl FromStr for $name {
            type Err = Error;

            fn from_str(s: &str) -> Result<Self, Error> {
                s.strip_prefix($sigil)
                    .filter(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
                    .and_then(|n| n.parse().ok())
                    .map(Self)
                    .ok_or_else(|| Error::InvalidId(s.to_owned()))
            }
        }
    };
}

tmux_id!(PaneId, '%', "pane");
tmux_id!(WindowId, '@', "window");
tmux_id!(SessionId, '$', "session");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_prints_ids() {
        assert_eq!("%12".parse::<PaneId>().unwrap(), PaneId::new(12));
        assert_eq!(PaneId::new(12).to_string(), "%12");
        assert_eq!("$0".parse::<SessionId>().unwrap().to_string(), "$0");
        assert_eq!("@7".parse::<WindowId>().unwrap().to_string(), "@7");
    }

    #[test]
    fn rejects_anything_that_is_not_an_id() {
        for bad in [
            "",
            "%",
            "12",
            "%-1",
            "%+1",
            "% 1",
            "%1;kill-server",
            "$1",
            "%99999999999",
            "%1\n",
        ] {
            assert!(bad.parse::<PaneId>().is_err(), "accepted {bad:?}");
        }
    }
}
