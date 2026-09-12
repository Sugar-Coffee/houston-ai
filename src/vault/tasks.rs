//! Tasks: markdown files that a view happens to read.
//!
//! The obvious worry about putting a task list in the vault is that somebody
//! will open one of these files, type into it, and break the view. It is worth
//! saying plainly why that is not the risk it looks like, because the answer
//! shapes everything in this module.
//!
//! **The file is the truth, and the parser cannot fail.** There is no
//! `Result` on [`Task::parse`] and no error state in the model. A file with no
//! frontmatter is an open task at normal priority. An unknown priority is
//! normal. A file with no heading takes its title from its filename. You
//! cannot write a markdown file that this refuses, so hand-editing cannot
//! break anything.
//!
//! **The real risk runs the other way**, and it is the one guarded here: an
//! app that owns the file rewrites it. Round-trip a hand-written note through
//! a naive save and you lose the field ordering, the comments, the extra keys
//! the person's Obsidian setup relies on. So Houston never rewrites a task.
//! [`with_field`] replaces one line and leaves every other byte alone, and a
//! test asserts exactly that.
//!
//! Which leaves the reason for keeping tasks as files at all: an agent can
//! read them. "Open a task for the thing you found" and "what am I meant to be
//! doing" both work without Houston running, and that is the whole point of a
//! vault that agents start in.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// The folder, relative to the vault root.
pub const FOLDER: &str = "Tasks";

/// Files in `Tasks/` that are documentation rather than work.
const NOT_TASKS: [&str; 1] = ["README.md"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    High,
    Normal,
    Low,
}

impl Priority {
    /// Anything unrecognised is normal, because the alternative is a task that
    /// does not appear in the list at all.
    pub fn read(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "high" | "urgent" | "p1" | "1" => Self::High,
            "low" | "someday" | "p3" | "3" => Self::Low,
            _ => Self::Normal,
        }
    }

    pub const fn key(self) -> &'static str {
        match self {
            Self::High => "high",
            Self::Normal => "normal",
            Self::Low => "low",
        }
    }

    /// A shape as well as a colour, so priority survives a colourblind reader
    /// and a monochrome terminal.
    pub const fn marker(self) -> &'static str {
        match self {
            Self::High => "▲",
            Self::Normal => "●",
            Self::Low => "▽",
        }
    }

    pub const fn next(self) -> Self {
        match self {
            Self::High => Self::Normal,
            Self::Normal => Self::Low,
            Self::Low => Self::High,
        }
    }

    pub const fn all() -> [Self; 3] {
        [Self::High, Self::Normal, Self::Low]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Status {
    Open,
    Done,
}

impl Status {
    /// Only an explicit "done" closes a task. Everything else — a typo, a
    /// status somebody invented like `blocked` — stays open and visible,
    /// because a task that quietly vanishes is worse than one in the wrong
    /// column.
    pub fn read(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "done" | "complete" | "completed" | "closed" | "true" => Self::Done,
            _ => Self::Open,
        }
    }

    pub const fn key(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Done => "done",
        }
    }

    pub const fn toggled(self) -> Self {
        match self {
            Self::Open => Self::Done,
            Self::Done => Self::Open,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Task {
    pub path: PathBuf,
    pub title: String,
    pub status: Status,
    pub priority: Priority,
    /// A folder name under `Projects/`, if the frontmatter names one. Not
    /// checked against what exists — a task can name a project you have not
    /// written up yet.
    pub project: Option<String>,
    pub tags: Vec<String>,
    /// Everything after the frontmatter, heading and all.
    pub body: String,
}

impl Task {
    /// Reads a task from a file. Unreadable files are skipped by the caller,
    /// which is the only failure this has.
    pub fn parse(path: &Path, source: &str) -> Self {
        let (front, body) = split(source);

        let title = heading(body)
            .or_else(|| front.and_then(|f| field(f, "title")).map(str::to_string))
            .unwrap_or_else(|| title_from_filename(path));

        Self {
            path: path.to_path_buf(),
            title,
            status: front.and_then(|f| field(f, "status")).map_or(Status::Open, Status::read),
            priority: front
                .and_then(|f| field(f, "priority"))
                .map_or(Priority::Normal, Priority::read),
            project: front
                .and_then(|f| field(f, "project"))
                .map(str::to_string)
                .filter(|value| !value.is_empty()),
            tags: front.map(tags).unwrap_or_default(),
            body: body.to_string(),
        }
    }

    /// The description: the body with its title heading removed, since the
    /// title is already on screen.
    /// Everything below the frontmatter, heading included. What the pane
    /// edits, and the only half of the file the editor can reach.
    pub fn body(&self) -> &str {
        self.body.trim()
    }

    pub fn description(&self) -> &str {
        let rest = &self.body[heading_end(&self.body).unwrap_or(0)..];
        rest.trim_start_matches(['\n', '\r'])
    }

    /// What to tell an agent that is being started on this task.
    ///
    /// Absolute paths throughout: the agent starts in the *project's* folder,
    /// where nothing vault-relative resolves.
    pub fn briefing(&self, vault_root: &Path) -> String {
        use std::fmt::Write as _;

        let mut prompt = format!("Work on this task: {}\n\nRead it first.", self.path.display());

        if let Some(project) = &self.project {
            let index = vault_root.join("Projects").join(project).join("index.md");
            if index.is_file() {
                let _ = write!(
                    prompt,
                    " Then read {} and the decisions/ beside it, before proposing anything.",
                    index.display()
                );
            }
        }

        let manual = vault_root.join("AGENTS.md");
        if manual.is_file() {
            let _ = write!(
                prompt,
                "\n\nWhen you are done, write back as {} describes.",
                manual.display()
            );
        }
        prompt.push('\n');
        prompt
    }
}

/// Splits frontmatter from body. Neither half is required.
fn split(source: &str) -> (Option<&str>, &str) {
    let Some(rest) = source.strip_prefix("---\n").or_else(|| source.strip_prefix("---\r\n")) else {
        return (None, source);
    };

    // An unterminated block is not frontmatter, it is a horizontal rule
    // somebody started the file with. Treating it as frontmatter would swallow
    // the whole note.
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        if matches!(line.trim_end(), "---" | "...") {
            return (Some(&rest[..offset]), &rest[offset + line.len()..]);
        }
        offset += line.len();
    }
    (None, source)
}

/// One scalar out of a frontmatter block. Deliberately not a YAML parser:
/// tasks use five keys, and a dependency that can fail on a malformed file is
/// the opposite of what this module promises.
fn field<'a>(front: &'a str, key: &str) -> Option<&'a str> {
    front.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        (name.trim() == key).then(|| unquote(value.trim()))
    })
}

fn unquote(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .or_else(|| value.strip_prefix('\'').and_then(|rest| rest.strip_suffix('\'')))
        .unwrap_or(value)
}

/// Tags in either shape people actually write: `tags: [a, b]` on one line, or
/// a `- ` list under `tags:`. Obsidian writes the second and humans write the
/// first.
fn tags(front: &str) -> Vec<String> {
    let mut lines = front.lines();
    let Some(rest) = lines.find_map(|line| {
        let (name, value) = line.split_once(':')?;
        (name.trim() == "tags").then_some(value.trim())
    }) else {
        return Vec::new();
    };

    let inline = rest.trim_start_matches('[').trim_end_matches(']');
    if !inline.trim().is_empty() {
        return inline.split(',').map(|tag| unquote(tag.trim()).to_string()).collect();
    }

    lines
        .map_while(|line| {
            let trimmed = line.trim();
            trimmed.starts_with("- ").then(|| unquote(trimmed[2..].trim()).to_string())
        })
        .filter(|tag| !tag.is_empty())
        .collect()
}

/// Byte offset just past the first `# ` heading line.
///
/// An offset rather than the line itself, because all three callers want to
/// slice around it: the title takes what is inside it, the description takes
/// what follows, and [`with_description`] keeps everything before it. The
/// previous version returned the line and callers used its *length* as the
/// offset, which is only the same number when the heading is the first line of
/// the body — two blank lines above it and the description lost a character.
fn heading_span(body: &str) -> Option<(usize, usize)> {
    let mut offset = 0;
    for line in body.split_inclusive('\n') {
        if line.trim_start().starts_with("# ") {
            return Some((offset, offset + line.len()));
        }
        offset += line.len();
    }
    None
}

fn heading_end(body: &str) -> Option<usize> {
    heading_span(body).map(|(_, end)| end)
}

fn heading(body: &str) -> Option<String> {
    let (start, end) = heading_span(body)?;
    Some(body[start..end].trim().trim_start_matches('#').trim().to_string())
}

/// `0003-fix-the-thing.md` reads as "Fix the thing". The number is filing, not
/// title, so it goes.
fn title_from_filename(path: &Path) -> String {
    let stem =
        path.file_stem().map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    let words = stem
        .split_once('-')
        .filter(|(number, _)| !number.is_empty() && number.chars().all(|c| c.is_ascii_digit()))
        .map_or(stem.as_str(), |(_, rest)| rest)
        .replace(['-', '_'], " ");

    let mut characters = words.trim().chars();
    characters.next().map_or_else(
        || "Untitled".to_string(),
        |first| first.to_uppercase().collect::<String>() + characters.as_str(),
    )
}

/// Rewrites one frontmatter field and nothing else.
///
/// `None` removes the field. Every other byte of the file — key order, blank
/// lines, comments, keys Houston has never heard of — comes out unchanged,
/// which is what makes it safe to point this at a note somebody else's tool
/// also writes.
pub fn with_field(source: &str, key: &str, value: Option<&str>) -> String {
    let (front, body) = split(source);

    let Some(front) = front else {
        // No frontmatter to edit. Removing a field it does not have is a
        // no-op; adding one means introducing a block, which is the only case
        // here that touches the shape of the file.
        let Some(value) = value else { return source.to_string() };
        return format!("---\n{key}: {value}\n---\n{source}");
    };

    let mut lines: Vec<String> = Vec::new();
    let mut replaced = false;

    for line in front.lines() {
        let matches = line.split_once(':').is_some_and(|(name, _)| name.trim() == key);

        if !matches {
            lines.push(line.to_string());
            continue;
        }
        replaced = true;
        if let Some(value) = value {
            lines.push(format!("{key}: {value}"));
        }
    }

    if !replaced && let Some(value) = value {
        lines.push(format!("{key}: {value}"));
    }

    format!("---\n{}\n---\n{body}", lines.join("\n"))
}

/// Replaces everything below the frontmatter, which is what the pane edits.
///
/// The heading goes in the editable half deliberately. A task's title *is* its
/// first heading — that is how [`Task::parse`] finds it — so making the
/// heading editable makes renaming free, with no second field and no key that
/// only exists for renaming. Delete the heading and the title falls back to
/// the filename, which is the same rule as every other malformed task here.
///
/// The frontmatter comes out byte-for-byte as it went in: the same promise
/// [`with_field`] makes, from the other side. It is the half you cannot reach
/// from the editor, and it is the half the view depends on.
pub fn with_body(source: &str, body: &str) -> String {
    let (front, _) = split(source);
    let body = body.trim();

    let mut out = String::new();
    if let Some(front) = front {
        out.push_str("---\n");
        out.push_str(front);
        out.push_str("---\n");
        // A blank line under the block, which is what every file this writes
        // already looks like. Without it a save reflows the whole vault the
        // first time it touches a note.
        if !body.is_empty() {
            out.push('\n');
        }
    }
    if !body.is_empty() {
        out.push_str(body);
        out.push('\n');
    }
    out
}

/// Writes a new body into the task on disk.
///
/// Splices against what is in the file *now* rather than against what was
/// there when the editor opened, so a priority somebody changed meanwhile — or
/// an agent adding a tag — survives being saved over.
pub fn set_body(path: &Path, body: &str) -> Result<()> {
    let source = std::fs::read_to_string(path)
        .with_context(|| format!("could not read {}", path.display()))?;
    std::fs::write(path, with_body(&source, body))
        .with_context(|| format!("could not write {}", path.display()))
}

/// The title a body would give a task, for showing a rename as it is typed.
///
/// The frontmatter `title:` fallback is missing on purpose: the editor does
/// not hold the frontmatter, so there is nothing to read it from. A task that
/// takes its title from there shows the filename while being edited and the
/// right thing the moment it is saved, which is a second of wrong in a case
/// that barely exists.
#[must_use]
pub fn title_from_body(path: &Path, body: &str) -> String {
    heading(body).unwrap_or_else(|| title_from_filename(path))
}

/// Every task in the vault, ordered for reading.
///
/// A missing `Tasks/` folder is an empty list, not an error: a vault somebody
/// pointed Houston at will not have one, and that is a thing to explain in the
/// view rather than a failure.
pub fn load(vault_root: &Path) -> Vec<Task> {
    let folder = vault_root.join(FOLDER);
    let Ok(entries) = std::fs::read_dir(&folder) else { return Vec::new() };

    let mut tasks: Vec<Task> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| is_task_file(path))
        .filter_map(|path| {
            let source = std::fs::read_to_string(&path).ok()?;
            Some(Task::parse(&path, &source))
        })
        .collect();

    // Open before done, then loudest first, then by filename so the order is
    // stable between refreshes — a list that reshuffles under the cursor is
    // unusable however good the sort is.
    tasks.sort_by(|a, b| {
        a.status.cmp(&b.status).then(a.priority.cmp(&b.priority)).then(a.path.cmp(&b.path))
    });
    tasks
}

fn is_task_file(path: &Path) -> bool {
    if path.extension().is_none_or(|extension| extension != "md") {
        return false;
    }
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else { return false };
    !name.starts_with('.') && !NOT_TASKS.contains(&name)
}

/// Writes a new task and returns its path.
pub fn create(
    vault_root: &Path,
    title: &str,
    priority: Priority,
    project: Option<&str>,
    tags: &[String],
) -> Result<PathBuf> {
    let title = title.trim();
    anyhow::ensure!(!title.is_empty(), "a task needs a title");

    let folder = vault_root.join(FOLDER);
    std::fs::create_dir_all(&folder)
        .with_context(|| format!("could not create {}", folder.display()))?;

    let name = format!("{:04}-{}.md", next_number(&folder), slug(title));
    let path = folder.join(name);

    let project = project
        .filter(|project| !project.is_empty())
        .map_or_else(String::new, |project| format!("project: {project}\n"));
    let tags =
        if tags.is_empty() { String::new() } else { format!("tags: [{}]\n", tags.join(", ")) };
    let source = format!(
        "---\nstatus: open\npriority: {}\n{project}{tags}---\n\n# {title}\n",
        priority.key()
    );

    std::fs::write(&path, source).with_context(|| format!("could not write {}", path.display()))?;
    Ok(path)
}

/// One past the highest number in the folder.
///
/// The highest rather than the count, because a folder holding 0001 and 0004
/// would otherwise put the next task at 0003 and collide the one after that.
/// Numbers do get reused once you delete the file that held them; the number
/// is a filing prefix, not an identity, and the alternative is a counter file
/// living in the vault for no reader's benefit.
fn next_number(folder: &Path) -> u32 {
    let Ok(entries) = std::fs::read_dir(folder) else { return 1 };
    entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let (number, _) = name.split_once('-')?;
            number.parse::<u32>().ok()
        })
        .max()
        .map_or(1, |highest| highest + 1)
}

fn slug(title: &str) -> String {
    let mut slug = String::new();
    for character in title.chars() {
        if character.is_ascii_alphanumeric() {
            slug.extend(character.to_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-');
    slug.chars().take(48).collect::<String>().trim_matches('-').to_string()
}

/// Applies a change to one field of a task on disk.
pub fn edit(path: &Path, key: &str, value: Option<&str>) -> Result<()> {
    let source = std::fs::read_to_string(path)
        .with_context(|| format!("could not read {}", path.display()))?;
    std::fs::write(path, with_field(&source, key, value))
        .with_context(|| format!("could not write {}", path.display()))
}

/// Folder names under `Projects/`, which is the list of projects a task can
/// belong to.
pub fn projects(vault_root: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(vault_root.join("Projects")) else { return Vec::new() };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| !name.starts_with('.') && !name.starts_with('_'))
        .collect();
    names.sort();
    names
}

/// Where a project's code actually lives, according to its `index.md`.
///
/// The convention the scaffold writes is a line saying **Where the code
/// lives:** followed by the path in backticks. Read rather than required: a
/// project without one just means the agent starts in the vault.
pub fn code_location(vault_root: &Path, project: &str) -> Option<PathBuf> {
    let index = vault_root.join("Projects").join(project).join("index.md");
    let source = std::fs::read_to_string(index).ok()?;

    let line = source.lines().find(|line| line.to_lowercase().contains("where the code lives"))?;
    let (_, after) = line.split_once('`')?;
    let (path, _) = after.split_once('`')?;

    let expanded = crate::paths::expand_home(Path::new(path.trim()));
    expanded.is_dir().then_some(expanded)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("houston-tasks-{name}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(FOLDER)).unwrap();
        root
    }

    fn parse(source: &str) -> Task {
        Task::parse(Path::new("/vault/Tasks/0001-something.md"), source)
    }

    /// The promise this module makes. If any markdown file can be a task, then
    /// editing one by hand cannot break the view — which is the whole argument
    /// for keeping tasks as files.
    #[test]
    fn any_markdown_file_at_all_is_a_valid_task() {
        for source in [
            "",
            "just a sentence",
            "---\nnot: closed\n\n# heading",
            "---\n---\n",
            "---\nstatus: banana\npriority: extremely\n---\n# Fine",
            "--- \nstatus: open\n---\n",
        ] {
            let task = parse(source);
            assert!(!task.title.is_empty(), "every file gets a title: {source:?}");
        }
    }

    /// A status somebody invented must not hide the task. Losing work is worse
    /// than showing it in the wrong place.
    #[test]
    fn an_unrecognised_status_leaves_the_task_open() {
        assert_eq!(parse("---\nstatus: blocked\n---\n# A").status, Status::Open);
        assert_eq!(parse("---\nstatus: DONE\n---\n# A").status, Status::Done);
    }

    #[test]
    fn a_file_with_no_frontmatter_is_an_open_task_at_normal_priority() {
        let task = parse("# Ring the bank\n\nBefore Friday.\n");
        assert_eq!(task.status, Status::Open);
        assert_eq!(task.priority, Priority::Normal);
        assert_eq!(task.title, "Ring the bank");
        assert_eq!(task.description().trim(), "Before Friday.");
        assert!(task.project.is_none());
    }

    #[test]
    fn a_file_with_no_heading_takes_its_title_from_its_name() {
        let task = Task::parse(Path::new("/v/Tasks/0007-fix-the-login-page.md"), "no heading");
        assert_eq!(task.title, "Fix the login page", "the number is filing, not title");
    }

    /// Obsidian writes list-form tags; humans write the inline form. Both are
    /// the same thing and both turn up in a real vault.
    #[test]
    fn tags_are_read_in_both_shapes_people_write_them() {
        assert_eq!(parse("---\ntags: [auth, tests]\n---\n").tags, ["auth", "tests"]);
        assert_eq!(parse("---\ntags:\n  - auth\n  - tests\n---\n").tags, ["auth", "tests"]);
        assert!(parse("---\ntags: []\n---\n").tags.is_empty());
    }

    /// An unterminated `---` at the top of a file is a horizontal rule, and
    /// treating it as frontmatter would swallow the note.
    #[test]
    fn an_unclosed_frontmatter_block_is_not_frontmatter() {
        let task = parse("---\n\n# A note that starts with a rule\n");
        assert!(task.body.starts_with("---"), "the whole file is body");
        assert_eq!(task.title, "A note that starts with a rule");
    }

    /// The failure mode this module exists to prevent: an app that owns the
    /// file normalises away everything it does not understand.
    #[test]
    fn editing_one_field_leaves_every_other_byte_alone() {
        let source = "---\n# a comment nothing here parses\ntitle: Mine\nstatus: open\nobsidian-only-key: 42\ntags:\n  - auth\n---\n\n# Mine\n\nBody   with  odd   spacing.\n";

        let after = with_field(source, "status", Some("done"));

        assert_eq!(
            after,
            source.replace("status: open", "status: done"),
            "one line changed and not one byte more"
        );
    }

    #[test]
    fn a_field_that_is_not_there_yet_is_appended_rather_than_reordering_the_rest() {
        let after = with_field("---\nstatus: open\n---\n# A\n", "project", Some("acme"));
        assert_eq!(after, "---\nstatus: open\nproject: acme\n---\n# A\n");
    }

    #[test]
    fn clearing_a_field_removes_the_line_instead_of_leaving_it_empty() {
        let after = with_field("---\nstatus: open\nproject: acme\n---\n# A\n", "project", None);
        assert_eq!(after, "---\nstatus: open\n---\n# A\n");
    }

    /// Adding frontmatter to a plain note must not eat the note.
    #[test]
    fn a_note_with_no_frontmatter_gains_a_block_and_keeps_its_text() {
        let after = with_field("# Ring the bank\n", "priority", Some("high"));
        assert_eq!(after, "---\npriority: high\n---\n# Ring the bank\n");
    }

    /// The frontmatter is the half the editor cannot reach, and it has to come
    /// out of a save exactly as it went in.
    #[test]
    fn rewriting_the_body_leaves_the_frontmatter_untouched() {
        let source =
            "---\nstatus: open\n# a comment\nobsidian-only-key: 42\n---\n\n# Alpha\n\nOld body.\n";

        let after = with_body(source, "# Renamed\n\nNew body.");

        assert_eq!(
            after,
            "---\nstatus: open\n# a comment\nobsidian-only-key: 42\n---\n\n# Renamed\n\nNew body.\n"
        );
        let task = Task::parse(Path::new("/v/Tasks/0001-a.md"), &after);
        assert_eq!(task.title, "Renamed", "the heading is the title, so this is a rename");
        assert_eq!(task.status, Status::Open, "and the metadata came through it");
    }

    /// Round-tripping without typing anything must not rewrite the file into a
    /// different shape, or opening and closing the editor would show as an edit.
    #[test]
    fn saving_a_body_unchanged_is_the_same_document() {
        for source in [
            "---\nstatus: open\n---\n\n# Alpha\n\nBody.\n",
            "# Alpha\n\nBody.\n",
            "---\nstatus: open\n---\n\n# Alpha\n",
            "just a sentence\n",
        ] {
            let task = Task::parse(Path::new("/v/Tasks/0001-a.md"), source);
            let after = with_body(source, task.body());

            assert_eq!(after, source, "an untouched task is byte-identical: {source:?}");
        }
    }

    /// Deleting the heading is allowed — it is markdown, and the parser has no
    /// error state. The title falls back to the filename, same as every other
    /// task with nothing to take a title from.
    #[test]
    fn deleting_the_heading_falls_back_to_the_filename_rather_than_breaking() {
        let after = with_body("---\nstatus: open\n---\n\n# Alpha\n\nBody.\n", "Body.");
        assert_eq!(after, "---\nstatus: open\n---\n\nBody.\n");

        let task = Task::parse(Path::new("/v/Tasks/0007-ring-the-bank.md"), &after);
        assert_eq!(task.title, "Ring the bank");
    }

    #[test]
    fn clearing_the_body_leaves_the_task_rather_than_an_empty_file() {
        let after = with_body("---\nstatus: open\n---\n\n# Alpha\n\nBody.\n", "   \n\n");
        assert_eq!(after, "---\nstatus: open\n---\n");
        assert_eq!(
            Task::parse(Path::new("/v/Tasks/0001-a.md"), &after).status,
            Status::Open,
            "an emptied task is still a task"
        );
    }

    /// What the sidebar shows while a rename is being typed.
    #[test]
    fn a_title_can_be_read_from_a_body_before_it_is_saved() {
        let path = Path::new("/v/Tasks/0007-ring-the-bank.md");
        assert_eq!(title_from_body(path, "# Halfway through a ren"), "Halfway through a ren");
        assert_eq!(title_from_body(path, "no heading yet"), "Ring the bank");
    }

    #[test]
    fn tasks_are_listed_open_first_then_by_priority() {
        let root = scratch("order");
        let write = |name: &str, body: &str| {
            std::fs::write(root.join(FOLDER).join(name), body).unwrap();
        };
        write("0001-low.md", "---\npriority: low\n---\n# Low\n");
        write("0002-done.md", "---\nstatus: done\npriority: high\n---\n# Done\n");
        write("0003-high.md", "---\npriority: high\n---\n# High\n");
        write("README.md", "# Tasks\n\nnot a task\n");

        let titles: Vec<String> = load(&root).into_iter().map(|task| task.title).collect();
        assert_eq!(titles, ["High", "Low", "Done"], "README.md is documentation, not work");

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_vault_with_no_tasks_folder_is_an_empty_list_rather_than_an_error() {
        let root = std::env::temp_dir().join("houston-tasks-absent");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        assert!(load(&root).is_empty());

        std::fs::remove_dir_all(&root).ok();
    }

    /// A gap in the numbering is normal — tasks get deleted. Counting files
    /// instead of reading the highest number would put the next task on top of
    /// one that already exists.
    #[test]
    fn a_gap_in_the_numbering_does_not_cause_a_collision() {
        let root = scratch("numbers");
        std::fs::write(root.join(FOLDER).join("0001-a.md"), "# A\n").unwrap();
        std::fs::write(root.join(FOLDER).join("0004-d.md"), "# D\n").unwrap();

        let next = create(&root, "New", Priority::Normal, None, &[]).unwrap();
        assert!(next.file_name().unwrap().to_string_lossy().starts_with("0005-"));
        assert_eq!(load(&root).len(), 3, "and nothing was overwritten");

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_created_task_reads_back_as_what_was_asked_for() {
        let root = scratch("create");
        let path = create(
            &root,
            "Fix the login page!",
            Priority::High,
            Some("acme-web"),
            &["auth".to_string()],
        )
        .unwrap();

        assert_eq!(
            path.file_name().unwrap().to_string_lossy(),
            "0001-fix-the-login-page.md",
            "punctuation does not belong in a filename"
        );

        let task = Task::parse(&path, &std::fs::read_to_string(&path).unwrap());
        assert_eq!(task.title, "Fix the login page!");
        assert_eq!(task.priority, Priority::High);
        assert_eq!(task.project.as_deref(), Some("acme-web"));
        assert_eq!(task.tags, ["auth"]);

        std::fs::remove_dir_all(&root).ok();
    }

    /// The one thing `Projects/<name>/index.md` has to do.
    #[test]
    fn a_project_says_where_its_code_lives_and_the_task_view_reads_it() {
        let root = scratch("location");
        let code = root.join("somewhere/acme");
        std::fs::create_dir_all(&code).unwrap();
        std::fs::create_dir_all(root.join("Projects/acme")).unwrap();
        std::fs::write(
            root.join("Projects/acme/index.md"),
            format!("# acme\n\n**Where the code lives:** `{}`\n", code.display()),
        )
        .unwrap();

        assert_eq!(code_location(&root, "acme"), Some(code));
        assert_eq!(code_location(&root, "nope"), None, "an unknown project is not an error");
        assert_eq!(projects(&root), ["acme"]);

        std::fs::remove_dir_all(&root).ok();
    }

    /// A path that has moved is worse than none: the agent would start in a
    /// directory that is not the project.
    #[test]
    fn a_recorded_path_that_no_longer_exists_is_not_offered() {
        let root = scratch("moved");
        std::fs::create_dir_all(root.join("Projects/gone")).unwrap();
        std::fs::write(
            root.join("Projects/gone/index.md"),
            "**Where the code lives:** `/definitely/not/here`\n",
        )
        .unwrap();

        assert_eq!(code_location(&root, "gone"), None);

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn the_briefing_points_an_agent_at_the_task_before_anything_else() {
        let root = scratch("briefing");
        let path = create(&root, "Do the thing", Priority::Normal, None, &[]).unwrap();
        let task = Task::parse(&path, &std::fs::read_to_string(&path).unwrap());

        let briefing = task.briefing(&root);
        assert!(briefing.contains(&path.display().to_string()), "by absolute path");
        assert!(
            briefing.lines().next().unwrap().contains("Work on this task"),
            "the first line is the instruction, since that is what a pasted prompt shows"
        );

        std::fs::remove_dir_all(&root).ok();
    }
}
