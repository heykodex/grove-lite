//! The tab list, independent of any webview. The app keeps one of these and
//! mirrors it onto real webviews.

use crate::input::display_url;
use serde::Serialize;
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// Nothing loaded yet. No webview exists, the window background shows.
    Blank,
    /// An ordinary web page.
    Web,
    /// The built-in history page.
    History,
}

#[derive(Debug, Clone)]
pub struct Tab {
    pub id: u32,
    pub kind: Kind,
    /// Full URL of a web tab, empty otherwise.
    pub url: String,
    pub title: String,
    pub loading: bool,
}

/// What the tab strip needs to draw one tab.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TabView {
    pub id: u32,
    pub kind: Kind,
    pub title: String,
    /// Address bar text.
    pub url: String,
    pub loading: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Snapshot {
    pub tabs: Vec<TabView>,
    pub active: u32,
}

#[derive(Debug, PartialEq, Eq)]
pub enum CloseOutcome {
    /// Unknown id, nothing changed.
    NotFound,
    /// That was the last tab.
    Empty,
    /// Closed; the active tab is now this one.
    Closed { active: u32 },
}

#[derive(Debug)]
pub struct Tabs {
    tabs: Vec<Tab>,
    active: u32,
    next_id: u32,
}

impl Default for Tabs {
    fn default() -> Self {
        Self::new()
    }
}

impl Tabs {
    /// One blank tab.
    pub fn new() -> Self {
        Self { tabs: vec![Self::blank(1)], active: 1, next_id: 2 }
    }

    fn blank(id: u32) -> Tab {
        Tab { id, kind: Kind::Blank, url: String::new(), title: String::new(), loading: false }
    }

    pub fn active_id(&self) -> u32 {
        self.active
    }

    pub fn active(&self) -> &Tab {
        self.get(self.active).expect("active tab always exists")
    }

    pub fn get(&self, id: u32) -> Option<&Tab> {
        self.tabs.iter().find(|t| t.id == id)
    }

    pub fn get_mut(&mut self, id: u32) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|t| t.id == id)
    }

    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    pub fn history_tab(&self) -> Option<u32> {
        self.tabs.iter().find(|t| t.kind == Kind::History).map(|t| t.id)
    }

    /// Add a blank tab at the end and make it active.
    pub fn open_blank(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.tabs.push(Self::blank(id));
        self.active = id;
        id
    }

    pub fn activate(&mut self, id: u32) -> bool {
        if self.get(id).is_some() {
            self.active = id;
            true
        } else {
            false
        }
    }

    pub fn close(&mut self, id: u32) -> CloseOutcome {
        let Some(index) = self.tabs.iter().position(|t| t.id == id) else {
            return CloseOutcome::NotFound;
        };
        self.tabs.remove(index);
        if self.tabs.is_empty() {
            return CloseOutcome::Empty;
        }
        if self.active == id {
            // The tab that slides into the closed slot, else the new last one.
            let next = index.min(self.tabs.len() - 1);
            self.active = self.tabs[next].id;
        }
        CloseOutcome::Closed { active: self.active }
    }

    fn active_index(&self) -> usize {
        self.tabs.iter().position(|t| t.id == self.active).unwrap_or(0)
    }

    pub fn next_tab(&mut self) -> u32 {
        let i = (self.active_index() + 1) % self.tabs.len();
        self.active = self.tabs[i].id;
        self.active
    }

    pub fn prev_tab(&mut self) -> u32 {
        let i = (self.active_index() + self.tabs.len() - 1) % self.tabs.len();
        self.active = self.tabs[i].id;
        self.active
    }

    /// Activate the `n`th tab (1-based). Returns `None` if there is none.
    pub fn select(&mut self, n: u8) -> Option<u32> {
        let tab = self.tabs.get((n as usize).checked_sub(1)?)?;
        self.active = tab.id;
        Some(self.active)
    }

    pub fn select_last(&mut self) -> u32 {
        self.active = self.tabs.last().expect("never empty").id;
        self.active
    }

    pub fn snapshot(&self) -> Snapshot {
        let tabs = self
            .tabs
            .iter()
            .map(|t| {
                let (title, url) = match t.kind {
                    Kind::Blank => (String::new(), String::new()),
                    Kind::History => ("history".to_string(), String::new()),
                    Kind::Web => {
                        let shown = Url::parse(&t.url).map(|u| display_url(&u)).unwrap_or_else(|_| t.url.clone());
                        let title = if t.title.trim().is_empty() { shown.clone() } else { t.title.trim().to_string() };
                        (title, shown)
                    }
                };
                TabView { id: t.id, kind: t.kind, title, url, loading: t.loading }
            })
            .collect();
        Snapshot { tabs, active: self.active }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn three() -> Tabs {
        let mut t = Tabs::new();
        t.open_blank();
        t.open_blank();
        t
    }

    #[test]
    fn starts_with_one_active_blank_tab() {
        let t = Tabs::new();
        assert_eq!(t.len(), 1);
        assert_eq!(t.active().kind, Kind::Blank);
        assert_eq!(t.active_id(), 1);
    }

    #[test]
    fn new_tabs_are_appended_and_activated() {
        let mut t = Tabs::new();
        assert_eq!(t.open_blank(), 2);
        assert_eq!(t.open_blank(), 3);
        assert_eq!(t.active_id(), 3);
        assert_eq!(t.snapshot().tabs.iter().map(|x| x.id).collect::<Vec<_>>(), vec![1, 2, 3]);
    }

    #[test]
    fn ids_are_never_reused() {
        let mut t = Tabs::new();
        t.open_blank();
        t.close(2);
        assert_eq!(t.open_blank(), 3);
    }

    #[test]
    fn closing_active_tab_picks_the_neighbour() {
        let mut t = three();
        t.activate(2);
        assert_eq!(t.close(2), CloseOutcome::Closed { active: 3 });
        assert_eq!(t.close(3), CloseOutcome::Closed { active: 1 });
    }

    #[test]
    fn closing_other_tab_keeps_active() {
        let mut t = three();
        t.activate(1);
        assert_eq!(t.close(3), CloseOutcome::Closed { active: 1 });
        assert_eq!(t.active_id(), 1);
    }

    #[test]
    fn closing_last_tab_reports_empty() {
        let mut t = Tabs::new();
        assert_eq!(t.close(1), CloseOutcome::Empty);
        assert_eq!(t.close(99), CloseOutcome::NotFound); // already empty
    }

    #[test]
    fn closing_unknown_tab_is_a_no_op() {
        let mut t = three();
        assert_eq!(t.close(42), CloseOutcome::NotFound);
        assert_eq!(t.len(), 3);
    }

    #[test]
    fn next_and_prev_wrap() {
        let mut t = three();
        t.activate(1);
        assert_eq!(t.prev_tab(), 3);
        assert_eq!(t.next_tab(), 1);
        assert_eq!(t.next_tab(), 2);
    }

    #[test]
    fn select_by_number() {
        let mut t = three();
        assert_eq!(t.select(1), Some(1));
        assert_eq!(t.select(3), Some(3));
        assert_eq!(t.select(4), None);
        assert_eq!(t.select(0), None);
        assert_eq!(t.active_id(), 3);
        t.activate(1);
        assert_eq!(t.select_last(), 3);
    }

    #[test]
    fn activate_unknown_fails() {
        let mut t = three();
        assert!(!t.activate(77));
        assert_eq!(t.active_id(), 3);
    }

    #[test]
    fn finds_history_tab() {
        let mut t = three();
        assert_eq!(t.history_tab(), None);
        t.get_mut(2).unwrap().kind = Kind::History;
        assert_eq!(t.history_tab(), Some(2));
    }

    #[test]
    fn snapshot_describes_each_kind() {
        let mut t = three();
        {
            let w = t.get_mut(1).unwrap();
            w.kind = Kind::Web;
            w.url = "https://example.com/".into();
            w.loading = true;
        }
        {
            let w = t.get_mut(2).unwrap();
            w.kind = Kind::Web;
            w.url = "https://example.com/a".into();
            w.title = "  Example A ".into();
        }
        t.get_mut(3).unwrap().kind = Kind::History;

        let s = t.snapshot();
        assert_eq!(s.active, 3);
        assert_eq!(s.tabs[0], TabView { id: 1, kind: Kind::Web, title: "example.com".into(), url: "example.com".into(), loading: true });
        assert_eq!(s.tabs[1].title, "Example A");
        assert_eq!(s.tabs[1].url, "example.com/a");
        assert_eq!(s.tabs[2].title, "history");
        assert_eq!(s.tabs[2].url, "");
    }

    #[test]
    fn snapshot_serializes_for_the_ui() {
        let json = serde_json::to_string(&Tabs::new().snapshot()).unwrap();
        assert_eq!(json, r#"{"tabs":[{"id":1,"kind":"blank","title":"","url":"","loading":false}],"active":1}"#);
    }
}
