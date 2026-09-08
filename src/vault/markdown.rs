//! Renders markdown into styled terminal lines.
//!
//! ADR-0004 keeps this deliberately modest: good enough to read comfortably,
//! not a competitor to Obsidian's renderer. Effort goes into the things that
//! aid navigation — headings that stand out, and `[[wikilinks]]` that are
//! visibly clickable targets — rather than typographic fidelity.
//!
//! Images and dataview blocks are shown as placeholders on purpose: measured
//! across the real vault, they appear in 4 and 1 files respectively
//! (`docs/research/vault-profile.md`).

use crate::ui::Theme;
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

/// A parsed note, ready to draw.
///
/// Parsing happens once when a note is opened, not once per frame. `log.md` in
/// the real vault is 184 KB; re-parsing that at 60fps would be absurd.
#[derive(Debug, Default)]
pub struct Document {
    pub lines: Vec<Line<'static>>,
    /// `[[wikilink]]` targets in document order, for the links panel.
    pub links: Vec<String>,
    /// Frontmatter `title:` if present, else the first heading.
    pub title: Option<String>,
}

impl Document {
    pub const fn len(&self) -> usize {
        self.lines.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

/// Where a run of text is going, which decides how it is styled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Block {
    Paragraph,
    Heading(u8),
    Code,
    Quote,
    Item,
    Metadata,
}

struct Renderer {
    theme: Theme,
    /// Wikilink targets lifted out before parsing, indexed by sentinel number.
    targets: Vec<String>,
    document: Document,
    /// Spans accumulated for the line currently being built.
    pending: Vec<Span<'static>>,
    block: Block,
    /// Inline styling from emphasis, strong, and inline code.
    inline: Modifier,
    code_inline: bool,
    /// Nesting depth and per-level counters for lists.
    list_stack: Vec<Option<u64>>,
    quote_depth: usize,
}

/// Marks where a wikilink was lifted out of the source. NUL is used because it
/// has no meaning to markdown and cannot occur in a sane note.
const SENTINEL: char = '\u{0}';

/// Lifts `[[wikilinks]]` out of the source before markdown parsing.
///
/// This has to happen *first*. `pulldown-cmark` treats `[` as the start of a
/// link reference, so by the time events arrive a `[[Deep Work]]` has already
/// been shredded into separate `Text` events and cannot be reassembled
/// reliably. Escaping the brackets does not help — it splits them too.
///
/// Each link becomes `\0<index>\0`, which markdown passes through untouched
/// as ordinary text, and which the renderer expands again on the way out.
fn extract_wikilinks(source: &str) -> (String, Vec<String>) {
    let source = source.replace(SENTINEL, "");
    let mut targets = Vec::new();
    let mut output = String::with_capacity(source.len());
    let mut rest = source.as_str();

    while let Some(open) = rest.find("[[") {
        let Some(close_offset) = rest[open + 2..].find("]]") else { break };
        let close = open + 2 + close_offset;

        output.push_str(&rest[..open]);
        output.push(SENTINEL);
        output.push_str(&targets.len().to_string());
        output.push(SENTINEL);

        targets.push(rest[open + 2..close].to_string());
        rest = &rest[close + 2..];
    }

    output.push_str(rest);
    (output, targets)
}

/// Parses `source` into drawable lines.
#[must_use]
pub fn parse(source: &str, theme: Theme) -> Document {
    let (source, targets) = extract_wikilinks(source);
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_YAML_STYLE_METADATA_BLOCKS);

    let mut renderer = Renderer {
        theme,
        targets,
        document: Document::default(),
        pending: Vec::new(),
        block: Block::Paragraph,
        inline: Modifier::empty(),
        code_inline: false,
        list_stack: Vec::new(),
        quote_depth: 0,
    };

    for event in Parser::new_ext(&source, options) {
        renderer.handle(event);
    }
    renderer.flush();

    let mut document = renderer.document;

    // A trailing separator after the last block is just wasted space.
    while document.lines.last().is_some_and(|line| line.spans.is_empty()) {
        document.lines.pop();
    }

    document
}

impl Renderer {
    fn handle(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => self.text(&text),
            Event::Code(code) => {
                self.code_inline = true;
                self.push_span(&format!(" {code} "));
                self.code_inline = false;
            }
            Event::SoftBreak => self.push_span(" "),
            Event::HardBreak => self.flush(),
            Event::Rule => {
                self.flush();
                self.document.lines.push(Line::from(Span::styled(
                    "─".repeat(48),
                    Style::default().fg(self.theme.dim),
                )));
                self.blank();
            }
            Event::TaskListMarker(done) => {
                let (glyph, colour) =
                    if done { ("☑ ", self.theme.accent) } else { ("☐ ", self.theme.dim) };
                self.pending.push(Span::styled(glyph, Style::default().fg(colour)));
            }
            Event::Html(html) | Event::InlineHtml(html) => {
                // Raw HTML in a note is almost always a comment or an embed;
                // showing the tags would be noise.
                let _ = html;
            }
            Event::FootnoteReference(name) => {
                self.push_span(&format!("[^{name}]"));
            }
            Event::InlineMath(text) | Event::DisplayMath(text) => self.push_span(&text),
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Heading { level, .. } => {
                self.flush();
                self.block = Block::Heading(heading_level(level));
            }
            Tag::Paragraph => self.block = Block::Paragraph,
            Tag::CodeBlock(kind) => {
                self.flush();
                self.block = Block::Code;
                if let CodeBlockKind::Fenced(language) = kind
                    && !language.is_empty()
                {
                    self.document.lines.push(Line::from(Span::styled(
                        format!("  {language}"),
                        Style::default().fg(self.theme.dim).add_modifier(Modifier::ITALIC),
                    )));
                }
            }
            Tag::BlockQuote(_) => {
                self.flush();
                self.quote_depth += 1;
                self.block = Block::Quote;
            }
            Tag::List(start) => {
                self.flush();
                self.list_stack.push(start);
            }
            Tag::Item => {
                self.flush();
                self.block = Block::Item;
                self.push_marker();
            }
            Tag::Emphasis => self.inline |= Modifier::ITALIC,
            // A table header row is bold for the same reason `**text**` is.
            Tag::Strong | Tag::TableHead => self.inline |= Modifier::BOLD,
            Tag::Strikethrough => self.inline |= Modifier::CROSSED_OUT,
            Tag::Link { .. } => self.inline |= Modifier::UNDERLINED,
            Tag::Image { dest_url, .. } => {
                // Terminal image protocols are out of scope (ADR-0004).
                self.push_span(&format!("🖼 {dest_url}"));
            }
            Tag::MetadataBlock(_) => {
                self.block = Block::Metadata;
            }
            Tag::TableCell => {
                if !self.pending.is_empty() {
                    self.pending.push(Span::styled(" │ ", Style::default().fg(self.theme.dim)));
                }
            }
            Tag::Table(_)
            | Tag::TableRow
            | Tag::FootnoteDefinition(_)
            | Tag::HtmlBlock
            | Tag::DefinitionList
            | Tag::DefinitionListTitle
            | Tag::DefinitionListDefinition
            | Tag::Superscript
            | Tag::Subscript => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Heading(_)
            | TagEnd::Paragraph
            | TagEnd::CodeBlock
            | TagEnd::MetadataBlock(_) => {
                self.flush();
                self.blank();
                self.block = Block::Paragraph;
            }
            TagEnd::BlockQuote(_) => {
                self.flush();
                self.quote_depth = self.quote_depth.saturating_sub(1);
                self.blank();
                self.block = Block::Paragraph;
            }
            TagEnd::List(_) => {
                self.flush();
                self.list_stack.pop();
                if self.list_stack.is_empty() {
                    self.blank();
                }
            }
            TagEnd::Item | TagEnd::TableRow | TagEnd::TableHead => self.flush(),
            TagEnd::Emphasis => self.inline.remove(Modifier::ITALIC),
            TagEnd::Strong => self.inline.remove(Modifier::BOLD),
            TagEnd::Strikethrough => self.inline.remove(Modifier::CROSSED_OUT),
            TagEnd::Link => self.inline.remove(Modifier::UNDERLINED),
            _ => {}
        }
    }

    /// Expands the sentinels left by `extract_wikilinks` back into styled
    /// links, and collects their targets in document order.
    ///
    /// Wikilinks are the vault's real navigation model — 158 files use them —
    /// so they get the accent colour and stand out as jump targets.
    fn text(&mut self, text: &str) {
        if self.block == Block::Metadata {
            self.metadata(text);
            return;
        }

        // A fenced block arrives as a single event with the newlines still in
        // it. Without splitting here every line of a code block collapses onto
        // one, which is how `TASK\nFROM "TASKS"` renders as `TASKFROM "TASKS"`.
        if self.block == Block::Code {
            let mut lines = text.split('\n').peekable();
            while let Some(line) = lines.next() {
                self.push_span(line);
                // Trailing newline: end the line, but do not add a blank one.
                if lines.peek().is_some() {
                    self.flush();
                }
            }
            if text.ends_with('\n') {
                self.flush();
            }
            return;
        }

        let mut rest = text;
        while let Some(open) = rest.find(SENTINEL) {
            let Some(close_offset) = rest[open + 1..].find(SENTINEL) else { break };
            let close = open + 1 + close_offset;

            self.push_span(&rest[..open]);

            // A sentinel with no matching target should be impossible; dropping
            // it silently beats rendering a raw control character.
            if let Some(target) = rest[open + 1..close]
                .parse::<usize>()
                .ok()
                .and_then(|index| self.targets.get(index))
                .cloned()
            {
                // Obsidian shows the alias half of `[[target|alias]]`.
                let shown = target.split_once('|').map_or(target.as_str(), |(_, alias)| alias);
                self.pending.push(Span::styled(
                    format!("[[{shown}]]"),
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD | self.inline),
                ));
                self.document.links.push(target);
            }

            rest = &rest[close + 1..];
        }
        self.push_span(rest);
    }

    /// Frontmatter, rendered as dim `key: value` lines rather than raw YAML.
    fn metadata(&mut self, text: &str) {
        for line in text.lines() {
            let trimmed = line.trim_end();
            if trimmed.is_empty() {
                continue;
            }
            if let Some(title) = trimmed.strip_prefix("title:")
                && self.document.title.is_none()
            {
                self.document.title = Some(title.trim().trim_matches('"').to_string());
            }
            self.document.lines.push(Line::from(Span::styled(
                trimmed.to_string(),
                Style::default().fg(self.theme.dim).add_modifier(Modifier::ITALIC),
            )));
        }
    }

    fn push_span(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.pending.push(Span::styled(text.to_string(), self.style()));
    }

    /// The bullet or number introducing a list item.
    fn push_marker(&mut self) {
        let depth = self.list_stack.len().saturating_sub(1);
        let indent = "  ".repeat(depth);

        let marker = match self.list_stack.last_mut() {
            Some(Some(number)) => {
                let current = *number;
                *number += 1;
                format!("{indent}{current}. ")
            }
            _ => format!("{indent}• "),
        };
        self.pending.push(Span::styled(marker, Style::default().fg(self.theme.dim)));
    }

    fn style(&self) -> Style {
        let base = match self.block {
            Block::Heading(1) => Style::default()
                .fg(self.theme.accent)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            Block::Heading(2) => {
                Style::default().fg(self.theme.accent).add_modifier(Modifier::BOLD)
            }
            Block::Heading(_) => Style::default().fg(self.theme.text).add_modifier(Modifier::BOLD),
            Block::Code => Style::default().fg(self.theme.text).bg(self.theme.surface),
            Block::Quote => Style::default().fg(self.theme.dim).add_modifier(Modifier::ITALIC),
            Block::Metadata => Style::default().fg(self.theme.dim),
            Block::Paragraph | Block::Item => Style::default().fg(self.theme.text),
        };

        if self.code_inline {
            return Style::default().fg(self.theme.accent).bg(self.theme.surface);
        }
        base.add_modifier(self.inline)
    }

    /// Ends the current line, if it has anything on it.
    fn flush(&mut self) {
        if self.pending.is_empty() {
            return;
        }

        let mut spans = Vec::with_capacity(self.pending.len() + 2);

        if let Block::Heading(level) = self.block {
            spans.push(Span::styled(
                format!("{} ", "#".repeat(level as usize)),
                Style::default().fg(self.theme.dim),
            ));
        }
        for _ in 0..self.quote_depth {
            spans.push(Span::styled("▌ ", Style::default().fg(self.theme.accent)));
        }
        if self.block == Block::Code {
            spans.push(Span::styled("  ", Style::default().bg(self.theme.surface)));
        }

        spans.append(&mut self.pending);
        self.document.lines.push(Line::from(spans));
    }

    /// A blank separator, never two in a row.
    fn blank(&mut self) {
        if self.document.lines.last().is_some_and(|line| line.spans.is_empty()) {
            return;
        }
        self.document.lines.push(Line::default());
    }
}

const fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(source: &str) -> Document {
        parse(source, Theme::default())
    }

    /// The visible text of a document, one string per line.
    fn text(document: &Document) -> Vec<String> {
        document
            .lines
            .iter()
            .map(|line| line.spans.iter().map(|span| span.content.as_ref()).collect())
            .collect()
    }

    #[test]
    fn headings_keep_their_hashes_as_a_level_cue() {
        let document = render("# Title\n\nBody text.");
        let lines = text(&document);
        assert!(lines.iter().any(|line| line.contains("# Title")));
        assert!(lines.iter().any(|line| line.contains("Body text.")));
    }

    #[test]
    fn wikilinks_are_collected_and_shown() {
        let document = render("See [[Deep Work]] and [[Projects/notes]].");
        assert_eq!(document.links, vec!["Deep Work", "Projects/notes"]);
        assert!(text(&document).concat().contains("[[Deep Work]]"));
    }

    #[test]
    fn an_aliased_wikilink_shows_the_alias_but_records_the_target() {
        let document = render("See [[real-note|what I called it]].");
        assert_eq!(document.links, vec!["real-note|what I called it"]);

        let rendered = text(&document).concat();
        assert!(rendered.contains("[[what I called it]]"), "the alias is what is shown");
        assert!(!rendered.contains("real-note"), "the target is not shown");
    }

    #[test]
    fn an_unclosed_wikilink_does_not_eat_the_rest_of_the_line() {
        let document = render("An unclosed [[link and more text after it.");
        assert!(document.links.is_empty());
        assert!(text(&document).concat().contains("more text after it"));
    }

    #[test]
    fn frontmatter_supplies_the_title() {
        let document = render("---\ntitle: My Note\ntags: [a, b]\n---\n\nBody.");
        assert_eq!(document.title.as_deref(), Some("My Note"));
    }

    #[test]
    fn task_list_markers_become_glyphs() {
        let document = render("- [ ] undone\n- [x] done");
        let rendered = text(&document).concat();
        assert!(rendered.contains('☐'));
        assert!(rendered.contains('☑'));
    }

    #[test]
    fn ordered_lists_number_themselves() {
        let document = render("1. first\n1. second\n1. third");
        let rendered = text(&document);
        assert!(rendered.iter().any(|line| line.contains("1. first")));
        assert!(rendered.iter().any(|line| line.contains("2. second")));
        assert!(rendered.iter().any(|line| line.contains("3. third")));
    }

    #[test]
    fn block_quotes_get_a_gutter_bar() {
        let document = render("> quoted wisdom");
        assert!(text(&document).concat().contains('▌'));
    }

    /// Every line of a fence has to survive as its own line. `pulldown-cmark`
    /// hands the whole block over in one event with the newlines embedded.
    #[test]
    fn code_fences_keep_their_line_breaks() {
        let document = render("```sh\nfirst\nsecond\nthird\n```");
        let lines = text(&document);

        let code = lines
            .iter()
            .filter(|line| {
                line.contains("first") || line.contains("second") || line.contains("third")
            })
            .count();

        assert_eq!(code, 3, "each line of the fence renders separately, got {lines:?}");
        assert!(!lines.iter().any(|line| line.contains("firstsecond")));
    }

    #[test]
    fn an_indented_code_block_also_keeps_its_lines() {
        let document = render("    one\n    two\n");
        let lines = text(&document);
        assert!(!lines.iter().any(|line| line.contains("onetwo")));
    }

    #[test]
    fn a_dataview_block_renders_as_an_ordinary_code_fence() {
        // One file in the whole vault uses these, so they get no special
        // handling — but they must not break the renderer either.
        let document = render("```dataview\nLIST FROM #project\n```");
        let rendered = text(&document).concat();
        assert!(rendered.contains("dataview"));
        assert!(rendered.contains("LIST FROM"));
    }

    #[test]
    fn images_degrade_to_a_placeholder() {
        let document = render("![alt](picture.png)");
        assert!(text(&document).concat().contains("picture.png"));
    }

    #[test]
    fn an_empty_document_is_empty_not_a_panic() {
        assert!(render("").is_empty());
        assert!(render("\n\n\n").is_empty());
    }

    #[test]
    fn blank_lines_never_double_up() {
        let document = render("Para one.\n\n\n\nPara two.");
        let blanks = document.lines.iter().filter(|line| line.spans.is_empty()).count();
        assert_eq!(blanks, 1, "one separator between the paragraphs, none trailing");
    }

    #[test]
    fn trailing_blank_lines_are_trimmed() {
        let document = render("Just one paragraph.\n\n\n");
        assert_eq!(document.len(), 1, "no wasted space at the end of a note");
    }

    /// A wikilink inside emphasis or a list item still has to survive, because
    /// markdown's own link syntax would otherwise claim the brackets.
    #[test]
    fn wikilinks_survive_being_nested_in_other_markdown() {
        let document = render("- see *[[Deep Work]]* now\n- and [[Other]]");
        assert_eq!(document.links, vec!["Deep Work", "Other"]);

        let rendered = text(&document).concat();
        assert!(rendered.contains("[[Deep Work]]"));
        assert!(!rendered.contains('\u{0}'), "no sentinel may leak into the output");
    }

    #[test]
    fn a_note_containing_a_nul_byte_does_not_confuse_the_sentinel() {
        let document = render("before \u{0}7\u{0} after [[Real]]");
        assert_eq!(document.links, vec!["Real"], "only the genuine link is collected");
    }
}
