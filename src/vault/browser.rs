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

/// The folder containing a relative path, or `""` at the top.
fn parent_of(relative: &str) -> &str {
    relative.rsplit_once('/').map_or("", |(parent, _)| parent)
}

/// One line of the sidebar, whichever view is showing.
///
/// Owned and rebuilt on change rather than borrowed per frame. Building it
/// per frame would mean a thousand allocations sixty times a second in a view
/// that changes when you press a key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    Folder { relative: String, name: String, depth: usize, expanded: bool },
    Note { id: NoteId, name: String, depth: usize },
}

impl Entry {
    #[must_use]
    pub const fn is_folder(&self) -> bool {
        matches!(self, Self::Folder { .. })
    }

    #[must_use]
    pub const fn depth(&self) -> usize {
        match self {
            Self::Folder { depth, .. } | Self::Note { depth, .. } => *depth,
        }
    }
}

pub struct Browser {
    pub vault: Vault,
    /// Which folders are open, for the browsing view.
    tree: crate::vault::tree::Tree,
    /// What the sidebar draws, rebuilt whenever it could have changed.
    entries: Vec<Entry>,
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
    wrap: bool,
}

impl Browser {
    pub fn new(vault: Vault) -> Self {
        let results = (0..vault.len().min(RESULT_LIMIT)).map(NoteId).collect();
        let mut browser = Self {
            vault,
            tree: crate::vault::tree::Tree::default(),
            entries: Vec::new(),
            matcher: Matcher::new(),
            source: Source::All,
            results,
            hits: Vec::new(),
            selected: 0,
            open: None,
            history: Vec::new(),
            mode: Mode::Browsing,
            query: String::new(),
            wrap: true,
        };
        browser.rebuild();
        browser
    }

    /// Rebuilds what the sidebar draws.
    ///
    /// One place, called after anything that could change it — the source, the
    /// results, an open folder, a file created or renamed. Everything else
    /// reads `entries`, so a view that disagrees with the vault is not
    /// something that can happen halfway through.
    pub fn rebuild(&mut self) {
        self.entries = if matches!(self.source, Source::All) {
            self.tree
                .rows(&self.vault)
                .into_iter()
                .map(|row| match row.note {
                    Some(id) => Entry::Note { id, name: row.name, depth: row.depth },
                    None => Entry::Folder {
                        relative: row.relative,
                        name: row.name,
                        depth: row.depth,
                        expanded: row.expanded,
                    },
                })
                .collect()
        } else {
            // Every other view is a list of notes and nothing else: a tree of
            // search results would hide the thing you searched for behind a
            // folder you then have to open.
            self.results
                .iter()
                .filter_map(|id| {
                    self.vault.get(*id).map(|note| Entry::Note {
                        id: *id,
                        name: note.stem.clone(),
                        depth: 0,
                    })
                })
                .collect()
        };

        self.selected = self.selected.min(self.entries.len().saturating_sub(1));
    }

    /// What the sidebar draws.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Opens or closes the selected folder.
    ///
    /// Returns whether it was a folder, so the caller can fall through to
    /// opening a note without asking twice.
    pub fn toggle_selected_folder(&mut self) -> bool {
        let Some(Entry::Folder { relative, .. }) = self.entries.get(self.selected) else {
            return false;
        };
        let relative = relative.clone();
        self.tree.toggle(&relative);
        self.rebuild();
        true
    }

    /// Closes the selected folder, or moves to the folder containing it.
    ///
    /// The `←` behaviour every tree has: pressing it on a closed thing takes
    /// you out a level rather than doing nothing.
    pub fn collapse_or_leave(&mut self) {
        let Some(entry) = self.entries.get(self.selected) else { return };

        if let Entry::Folder { relative, expanded: true, .. } = entry {
            let relative = relative.clone();
            self.tree.collapse(&relative);
            return self.rebuild();
        }

        // Walk up the list to the nearest row one level shallower.
        let depth = entry.depth();
        if depth == 0 {
            return;
        }
        if let Some(parent) =
            self.entries[..self.selected].iter().rposition(|other| other.depth() < depth)
        {
            self.selected = parent;
        }
    }

    /// The path the selection points at, relative to the vault root.
    pub fn selected_relative(&self) -> Option<String> {
        match self.entries.get(self.selected)? {
            Entry::Folder { relative, .. } => Some(relative.clone()),
            Entry::Note { id, .. } => self.vault.get(*id).map(|note| note.relative.clone()),
        }
    }

    /// The absolute path the selection points at.
    pub fn selected_path(&self) -> Option<std::path::PathBuf> {
        Some(self.vault.root().join(self.selected_relative()?))
    }

    /// Whether the selection is a folder.
    pub fn selection_is_folder(&self) -> bool {
        self.entries.get(self.selected).is_some_and(Entry::is_folder)
    }

    /// The folder a new thing should go into: the selected folder, or the one
    /// containing the selected note.
    pub fn target_folder(&self) -> String {
        match self.entries.get(self.selected) {
            Some(Entry::Folder { relative, .. }) => relative.clone(),
            Some(Entry::Note { id, .. }) => {
                self.vault.get(*id).map(|note| note.folder().to_string()).unwrap_or_default()
            }
            None => String::new(),
        }
    }

    /// Re-reads the vault from disk and puts the selection back where it was.
    ///
    /// Called after anything that changes the files. `reveal` opens the
    /// folders down to `focus` so a note created three levels deep is visible
    /// rather than hidden behind a closed folder — which is what makes "new
    /// note" look like it did nothing.
    pub fn reindex(&mut self, focus: Option<&str>) -> Result<()> {
        let root = self.vault.root().to_path_buf();
        self.vault = Vault::open(root)?;

        if let Some(focus) = focus {
            self.tree.reveal(parent_of(focus));
        }

        self.source = Source::All;
        self.results = (0..self.vault.len().min(RESULT_LIMIT)).map(NoteId).collect();
        self.hits.clear();
        self.rebuild();

        if let Some(focus) = focus
            && let Some(index) = self.entries.iter().position(|entry| match entry {
                Entry::Folder { relative, .. } => relative == focus,
                Entry::Note { id, .. } => {
                    self.vault.get(*id).is_some_and(|note| note.relative == focus)
                }
            })
        {
            self.selected = index;
        }
        Ok(())
    }

    /// Whether long lines are wrapped.
    ///
    /// Prose wants wrapping; a wide table does not, because wrapping destroys
    /// the column alignment that makes it readable. `ratatui`'s `Paragraph`
    /// applies one policy to the whole document, so until the renderer does
    /// its own per-line wrapping this is a toggle rather than a decision.
    pub const fn wraps(&self) -> bool {
        self.wrap
    }

    pub const fn toggle_wrap(&mut self) {
        self.wrap = !self.wrap;
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
        match self.entries.get(self.selected)? {
            Entry::Note { id, .. } => self.vault.get(*id),
            Entry::Folder { .. } => None,
        }
    }

    pub const fn select_next(&mut self) {
        if !self.entries.is_empty() {
            self.selected = (self.selected + 1) % self.entries.len();
        }
    }

    pub const fn select_previous(&mut self) {
        if !self.entries.is_empty() {
            self.selected = (self.selected + self.entries.len() - 1) % self.entries.len();
        }
    }

    /// Opens the selected note, remembering where we came from.
    pub fn open_selected(&mut self, theme: Theme) -> Result<()> {
        // A folder opens in the sense a folder can: it shows what is inside.
        if self.toggle_selected_folder() {
            return Ok(());
        }
        let Some(Entry::Note { id, .. }) = self.entries.get(self.selected) else { return Ok(()) };
        let id = *id;

        // A query has done its job once you have picked from it. Leaving the
        // list filtered means the sidebar keeps answering a question you have
        // already finished asking, and — worse — hides where the note you just
        // opened actually lives.
        //
        // Links and backlinks are left alone: those are a list you traverse,
        // and folding it away after one selection would make following a
        // chain of links impossible.
        if matches!(self.source, Source::Filtered(_) | Source::Grep(_)) {
            self.show_in_tree(id);
        }

        self.open_note(id, theme)
    }

    /// Returns to the tree with a note revealed and selected.
    ///
    /// The point is orientation: you asked for a name, and the answer is worth
    /// more when you can see which folder it came out of.
    fn show_in_tree(&mut self, id: NoteId) {
        let Some(relative) = self.vault.get(id).map(|note| note.relative.clone()) else { return };

        self.query.clear();
        self.hits.clear();
        self.source = Source::All;
        self.results = (0..self.vault.len().min(RESULT_LIMIT)).map(NoteId).collect();
        self.tree.reveal(parent_of(&relative));
        self.rebuild();

        if let Some(index) = self
            .entries
            .iter()
            .position(|entry| matches!(entry, Entry::Note { id: other, .. } if *other == id))
        {
            self.selected = index;
        }
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
        self.rebuild();
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
        self.rebuild();
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
        self.rebuild();
    }

    fn refresh_filter(&mut self) {
        self.results = self.matcher.search(&self.vault, &self.query, RESULT_LIMIT);
        self.source = Source::Filtered(self.query.clone());
        self.hits.clear();
        self.selected = 0;
        self.rebuild();
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

    /// Picking a result answers the question, so the filter comes off and the
    /// tree shows you where the answer lives.
    #[test]
    fn choosing_a_find_result_returns_to_the_tree_with_it_revealed() {
        let root = scratch(
            "findreveal",
            &[("Projects/deep/kickoff.md", "# kickoff"), ("other.md", "# other")],
        );
        let mut browser = browser(&root);

        browser.begin_find();
        for character in "kickoff".chars() {
            browser.push_query(character);
        }
        browser.end_query();

        assert_eq!(browser.entries().len(), 1, "the filter narrowed to one");

        browser.open_selected(Theme::default()).unwrap();

        // Back to every top-level row, plus the folders opened on the way.
        let names: Vec<String> = browser
            .entries()
            .iter()
            .map(|entry| match entry {
                Entry::Folder { name, .. } | Entry::Note { name, .. } => name.clone(),
            })
            .collect();
        assert_eq!(
            names,
            vec!["Projects", "deep", "kickoff", "other"],
            "the whole vault is back, with the path to the note opened"
        );

        assert_eq!(
            browser.selected_relative().as_deref(),
            Some("Projects/deep/kickoff.md"),
            "and the selection is on what you picked, so you can see where it is"
        );
        assert!(browser.query().is_empty(), "the query is spent");

        std::fs::remove_dir_all(&root).ok();
    }

    /// Links are a list you traverse. Folding it away after one selection
    /// would make following a chain of them impossible.
    #[test]
    fn following_a_link_leaves_the_links_list_alone() {
        let root = scratch(
            "linkstay",
            &[
                ("hub.md", "# hub\n\n[[one]] and [[two]]\n"),
                ("one.md", "# one"),
                ("two.md", "# two"),
            ],
        );
        let mut browser = browser(&root);

        // Open the hub so its links are recorded, then list them.
        let hub = browser
            .entries()
            .iter()
            .find_map(|entry| match entry {
                Entry::Note { id, name, .. } if name == "hub" => Some(*id),
                _ => None,
            })
            .unwrap();
        browser.open_note(hub, Theme::default()).unwrap();
        assert!(browser.show_links(), "the hub has links");

        let before = browser.entries().len();
        browser.open_selected(Theme::default()).unwrap();

        assert_eq!(
            browser.entries().len(),
            before,
            "still the links list, so the next one is one keypress away"
        );

        std::fs::remove_dir_all(&root).ok();
    }

    /// A note made three folders deep must end up visible, or "new note"
    /// looks like it did nothing.
    #[test]
    fn reindexing_reveals_the_thing_that_was_just_made() {
        let root = scratch("reveal", &[("top.md", "# top")]);
        let mut browser = browser(&root);

        assert_eq!(browser.entries().len(), 1, "one note, no folders yet");

        let made = crate::vault::files::create_note(&root, "a/b/deep").unwrap();
        let relative = made.strip_prefix(&root).unwrap().to_string_lossy().into_owned();
        browser.reindex(Some(&relative)).unwrap();

        let names: Vec<String> = browser
            .entries()
            .iter()
            .map(|entry| match entry {
                Entry::Folder { name, .. } | Entry::Note { name, .. } => name.clone(),
            })
            .collect();

        assert_eq!(names, vec!["a", "b", "deep", "top"], "every folder on the way is open");
        assert_eq!(
            browser.selected_relative().as_deref(),
            Some("a/b/deep.md"),
            "and the selection is on what was just made"
        );

        std::fs::remove_dir_all(&root).ok();
    }

    /// Left on a closed folder steps out rather than doing nothing.
    #[test]
    fn collapsing_walks_out_a_level_when_there_is_nothing_to_close() {
        let root = scratch("collapse", &[("folder/inner.md", "# inner")]);
        let mut browser = browser(&root);

        browser.toggle_selected_folder();
        browser.select_next();
        assert_eq!(browser.selected_relative().as_deref(), Some("folder/inner.md"));

        browser.collapse_or_leave();
        assert_eq!(
            browser.selected_relative().as_deref(),
            Some("folder"),
            "from a note, left goes to the folder holding it"
        );

        browser.collapse_or_leave();
        assert!(
            !browser.entries().iter().any(|e| matches!(e, Entry::Note { .. })),
            "and again closes that folder"
        );

        std::fs::remove_dir_all(&root).ok();
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

        assert_eq!(browser.entries().len(), 3);

        browser.begin_find();
        for character in "pay".chars() {
            browser.push_query(character);
        }
        assert_eq!(browser.entries().len(), 1);
        assert_eq!(browser.selected_note().unwrap().stem, "payments");

        browser.pop_query();
        browser.pop_query();
        browser.pop_query();
        assert_eq!(browser.entries().len(), 3, "clearing the query restores everything");

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

        assert_eq!(browser.entries().len(), 1);
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
        assert!(browser.entries().is_empty());
        browser.select_next();
        browser.select_previous();
        assert!(browser.selected_note().is_none(), "no selection, but no panic either");

        fs::remove_dir_all(&root).ok();
    }
}
