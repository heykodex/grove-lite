//! Keyboard actions. The page-side script (`ui/keys.js`) sends one of these
//! names; anything else is rejected, so a web page can only ever ask for this
//! short, harmless list.

use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    NewTab,
    CloseTab,
    NextTab,
    PrevTab,
    FocusUrl,
    History,
    Reload,
    Back,
    Forward,
    /// 1-based tab number, 1 to 8.
    Select(u8),
    SelectLast,
}

impl FromStr for Action {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, ()> {
        Ok(match s {
            "new_tab" => Action::NewTab,
            "close_tab" => Action::CloseTab,
            "next_tab" => Action::NextTab,
            "prev_tab" => Action::PrevTab,
            "focus_url" => Action::FocusUrl,
            "history" => Action::History,
            "reload" => Action::Reload,
            "back" => Action::Back,
            "forward" => Action::Forward,
            "select:last" => Action::SelectLast,
            other => {
                let n: u8 = other.strip_prefix("select:").ok_or(())?.parse().map_err(|_| ())?;
                if (1..=8).contains(&n) {
                    Action::Select(n)
                } else {
                    return Err(());
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_actions() {
        assert_eq!("new_tab".parse(), Ok(Action::NewTab));
        assert_eq!("history".parse(), Ok(Action::History));
        assert_eq!("select:1".parse(), Ok(Action::Select(1)));
        assert_eq!("select:8".parse(), Ok(Action::Select(8)));
        assert_eq!("select:last".parse(), Ok(Action::SelectLast));
    }

    #[test]
    fn rejects_everything_else() {
        for bad in ["", "NEW_TAB", "navigate", "select:0", "select:9", "select:-1", "select:", "select:x", "new_tab ", "quit"] {
            assert!(bad.parse::<Action>().is_err(), "{bad:?} should be rejected");
        }
    }
}
