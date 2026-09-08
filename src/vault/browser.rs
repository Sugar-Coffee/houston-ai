//! Vault navigation state: what is listed, what is open, and where you came
//! from.
//!
//! ADR-0004 — the measure of this module is how fast it gets a note's path into
//! an agent session, not how nicely it renders.

use super::{
    Matcher, Note, NoteId, Vault,
    markdown::{self, Document},
    search::{self, Hit},
};
use crate::ui::Theme;
use anyhow::Result;

/// How many results a picker will show. Beyond this, refine the query.
const RESULT_LIMIT: usize = 200;

/// What the left-hand list is currently showing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// Every note in the vault.
    All,
    /// Fuzzy matches for a query.
    Filtered(String),
    /// Notes the open note links to.
    Links,
    /// Notes that link to the open note.
    Backlinks,
    /// Full-text hits for a query.
    Grep(String),
}

impl Source {
    pub fn label(&self) -> String {
        match self {
            Self::All => " all notes ".to_string(),
            Self::Filtered(query) => format!(" find: {query} "),
            Self::Links => " links out ".to_string(),
            Self::Backlinks => " backlinks ".to_string(),
            Self::Grep(query) => format!(" search: {query} "),
        }
    }
}

/// Whether keystrokes move the selection or edit a query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Browsing,
    /// Typing a fuzzy query, filtering as you go.
    Finding,
    /// Typing a full-text query, run on Enter.
    Searching,
}

/// A note that has been parsed and is on screen.
pub struct Open {
    pub id: NoteId,
    pub document: Document,
    pub scroll: u16,
}

pub struct Browser {
    pub vault: Vault,
    matcher: Matcher,
    source: Source,
    results: Vec<NoteId>,
    /// Full-text hits, kept alongside results so line numbers can be shown.
    hits: Vec<Hit>,
    selected: usize,
    open: Option<Open>,
    /// Notes visited, so following a wikilink can be undone.
    history: Vec<NoteId>,
    mode: Mode,
    query: String,
}

impl Browser {
    pub fn new(vault: Vault) -> Self {
        let results = (0..vault.len().min(RESULT_LIMIT)).map(NoteId).collect();
        Self {
            vault,
            matcher: Matcher::new(),
            source: Source::All,
            results,
            hits: Vec::new(),
            selected: 0,
            open: None,
            history: Vec::new(),
            mode: Mode::Browsing,
            query: String::new(),
        }
    }

    pub const fn mode(&self) -> Mode {
        self.mode
    }

    pub const fn source(&self) -> &Source {
        &self.source
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn results(&self) -> &[NoteId] {
        &self.results
    }

    pub const fn selected_index(&self) -> usize {
        self.selected
    }

    pub fn hits(&self) -> &[Hit] {
        &self.hits
    }

    pub const fn open(&self) -> Option<&Open> {
        self.open.as_ref()
    }

    pub fn selected_note(&self) -> Option<&Note> {
        self.results.get(self.selected).and_then(|id| self.vault.get(*id))
    }

    pub const fn select_next(&mut self) {
        if !self.results.is_empty() {
            self.selected = (self.selected + 1) % self.results.len();
        }
    }

    pub const fn select_previous(&mut self) {
        if !self.results.is_empty() {
            self.selected = (self.selected + self.results.len() - 1) % self.results.len();
        }
    }

    /// Opens the selected note, remembering where we came from.
    pub fn open_selected(&mut self, theme: Theme) -> Result<()> {
        let Some(id) = self.results.get(self.selected).copied() else { return Ok(()) };
        self.open_note(id, theme)
    }

    pub fn open_note(&mut self, id: NoteId, theme: Theme) -> Result<()> {
        if let Some(current) = self.open.as_ref().map(|open| open.id)
            && current != id
        {
            self.history.push(current);
        }

        let Some(note) = self.vault.get(id) else { return Ok(()) };
        let body = std::fs::read_to_string(&note.path)?;
        let document = markdown::parse(&body, theme);

        // Recording links here is what makes backlinks work at all: the graph
        // is built from notes actually visited, rather than by parsing all
        // 1,000 files at startup.
        self.vault.record_links(id, &document.links);

        self.open = Some(Open { id, document, scroll: 0 });
        Ok(())
    }

    /// Returns to the previously open note. `true` if there was one.
    pub fn go_back(&mut self, theme: Theme) -> bool {
        let Some(previous) = self.history.pop() else { return false };
        // Opening would push the current note back onto the history and make
        // `back` bounce between two notes forever.
        let restored = self.load(previous, theme);
        restored.is_ok()
    }

    fn load(&mut self, id: NoteId, theme: Theme) -> Result<()> {
        let Some(note) = self.vault.get(id) else { return Ok(()) };
        let body = std::fs::read_to_string(&note.path)?;
        let document = markdown::parse(&body, theme);
        self.vault.record_links(id, &document.links);
        self.open = Some(Open { id, document, scroll: 0 });
        Ok(())
    }

    pub fn scroll(&mut self, delta: i32) {
        let Some(open) = self.open.as_mut() else { return };
        let last = u16::try_from(open.document.len().saturating_sub(1)).unwrap_or(u16::MAX);
        let target = i64::from(open.scroll) + i64::from(delta);
        open.scroll = u16::try_from(target.clamp(0, i64::from(last))).unwrap_or(0);
    }

    pub const fn scroll_to_top(&mut self) {
        if let Some(open) = self.open.as_mut() {
            open.scroll = 0;
        }
    }

    pub fn scroll_to_bottom(&mut self) {
        if let Some(open) = self.open.as_mut() {
            open.scroll = u16::try_from(open.document.len().saturating_sub(1)).unwrap_or(u16::MAX);
        }
    }

    // --- list sources ---

    pub fn show_all(&mut self) {
        self.source = Source::All;
        self.results = (0..self.vault.len().min(RESULT_LIMIT)).map(NoteId).collect();
        self.hits.clear();
        self.selected = 0;
    }

    /// Lists the wikilinks going out of the open note.
    pub fn show_links(&mut self) -> bool {
        let Some(open) = self.open.as_ref() else { return false };
        let links = self.vault.outgoing(open.id).to_vec();
        if links.is_empty() {
            return false;
        }
        self.source = Source::Links;
        self.results = links;
        self.hits.clear();
        self.selected = 0;
        true
    }

    /// Lists the notes linking to the open note, among those visited so far.
    pub fn show_backlinks(&mut self) -> bool {
        let Some(open) = self.open.as_ref() else { return false };
        let links = self.vault.backlinks(open.id);
        if links.is_empty() {
            return false;
        }
        self.source = Source::Backlinks;
        self.results = links;
        self.hits.clear();
        self.selected = 0;
        true
    }

    // --- query modes ---

    pub fn begin_find(&mut self) {
        self.mode = Mode::Finding;
        self.query.clear();
        self.refresh_filter();
    }

    pub fn begin_search(&mut self) {
        self.mode = Mode::Searching;
        self.query.clear();
    }

    /// Leaves a query mode, keeping whatever results it produced.
    pub const fn end_query(&mut self) {
        self.mode = Mode::Browsing;
    }

    pub fn push_query(&mut self, character: char) {
        self.query.push(character);
        if self.mode == Mode::Finding {
            self.refresh_filter();
        }
    }

    pub fn pop_query(&mut self) {
        self.query.pop();
        if self.mode == Mode::Finding {
            self.refresh_filter();
        }
    }

    /// Runs a full-text search. Only meaningful in `Searching` mode.
    pub fn run_search(&mut self) {
        self.hits = search::grep(&self.vault, &self.query, RESULT_LIMIT);
        self.results = self.hits.iter().map(|hit| hit.note).collect();
        self.source = Source::Grep(self.query.clone());
        self.selected = 0;
        self.mode = Mode::Browsing;
    }

    fn refresh_filter(&mut self) {
        self.results = self.matcher.search(&self.vault, &self.query, RESULT_LIMIT);
        self.source = Source::Filtered(self.query.clone());
        self.hits.clear();
        self.selected = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("houston-browser-{name}"));
        let _ = fs::remove_dir_all(&root);
        for (path, body) in files {
            let full = root.join(path);
            fs::create_dir_all(full.parent().unwrap()).unwrap();
            fs::write(full, body).unwrap();
        }
        root
    }

    fn browser(root: &std::path::Path) -> Browser {
        Browser::new(Vault::open(root).unwrap())
    }

    #[test]
    fn opening_a_note_parses_it_and_records_its_links() {
        let root = scratch("open", &[("a.md", "# A\n\nsee [[b]]"), ("b.md", "# B")]);
        let mut browser = browser(&root);

        browser.open_selected(Theme::default()).unwrap();
        let open = browser.open().expect("a note should be open");
        assert!(!open.document.is_empty());
        assert_eq!(open.document.links, vec!["b"]);

        // The link is resolved, so backlinks work from b's side.
        let b = browser.vault.resolve_link("b").unwrap();
        assert_eq!(browser.vault.backlinks(b).len(), 1);

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn going_back_returns_to_the_previous_note_and_does_not_bounce() {
        let root = scratch("history", &[("a.md", "A"), ("b.md", "B")]);
        let mut browser = browser(&root);
        let theme = Theme::default();

        let a = browser.vault.resolve_link("a").unwrap();
        let b = browser.vault.resolve_link("b").unwrap();

        browser.open_note(a, theme).unwrap();
        browser.open_note(b, theme).unwrap();
        assert_eq!(browser.open().unwrap().id, b);

        assert!(browser.go_back(theme));
        assert_eq!(browser.open().unwrap().id, a);

        // Going back again must not return to b: history is a stack, not a
        // toggle between the last two notes.
        assert!(!browser.go_back(theme), "history should now be empty");
        assert_eq!(browser.open().unwrap().id, a);

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn filtering_narrows_the_list_as_you_type() {
        let root =
            scratch("filter", &[("payments.md", ""), ("scheduler.md", ""), ("onboarding.md", "")]);
        let mut browser = browser(&root);

        assert_eq!(browser.results().len(), 3);

        browser.begin_find();
        for character in "cog".chars() {
            browser.push_query(character);
        }
        assert_eq!(browser.results().len(), 1);
        assert_eq!(browser.selected_note().unwrap().stem, "payments");

        browser.pop_query();
        browser.pop_query();
        browser.pop_query();
        assert_eq!(browser.results().len(), 3, "clearing the query restores everything");

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn full_text_search_finds_content_not_filenames() {
        let root = scratch("grep", &[("a.md", "nothing here"), ("b.md", "the SECRET word")]);
        let mut browser = browser(&root);

        browser.begin_search();
        for character in "secret".chars() {
            browser.push_query(character);
        }
        browser.run_search();

        assert_eq!(browser.results().len(), 1);
        assert_eq!(browser.selected_note().unwrap().stem, "b");
        assert_eq!(browser.hits()[0].line, 1);
        assert_eq!(browser.mode(), Mode::Browsing, "running a search leaves query mode");

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn scrolling_is_clamped_to_the_document() {
        let root = scratch("scroll", &[("a.md", "one\n\ntwo\n\nthree")]);
        let mut browser = browser(&root);
        browser.open_selected(Theme::default()).unwrap();

        browser.scroll(-50);
        assert_eq!(browser.open().unwrap().scroll, 0, "cannot scroll above the first line");

        browser.scroll(10_000);
        let length = browser.open().unwrap().document.len();
        assert_eq!(browser.open().unwrap().scroll as usize, length - 1, "clamped to the last line");

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn link_and_backlink_views_refuse_to_open_when_empty() {
        let root = scratch("empty-links", &[("a.md", "no links here")]);
        let mut browser = browser(&root);
        browser.open_selected(Theme::default()).unwrap();

        assert!(!browser.show_links(), "an empty link list should not replace the view");
        assert!(!browser.show_backlinks());
        assert_eq!(*browser.source(), Source::All, "the list source is unchanged");

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn selection_wraps_and_survives_an_empty_result_set() {
        let root = scratch("wrap", &[("a.md", ""), ("b.md", "")]);
        let mut browser = browser(&root);

        browser.select_previous();
        assert_eq!(browser.selected_index(), 1);
        browser.select_next();
        assert_eq!(browser.selected_index(), 0);

        browser.begin_find();
        for character in "zzzznomatch".chars() {
            browser.push_query(character);
        }
        assert!(browser.results().is_empty());
        browser.select_next();
        browser.select_previous();
        assert!(browser.selected_note().is_none(), "no selection, but no panic either");

        fs::remove_dir_all(&root).ok();
    }
}
