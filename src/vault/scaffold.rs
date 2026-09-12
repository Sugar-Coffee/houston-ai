//! What a brand new vault contains.
//!
//! **The vault is where agents start, not a folder they read from.** The way
//! this gets used is: an agent launched in the vault reads `CLAUDE.md`, learns
//! that `Projects/acme-api/index.md` says the code is at `~/work/acme-api`,
//! and goes there already knowing what the project is and what has been
//! decided about it. That only works if the vault has a shape an agent can
//! rely on, and a brand new one has no shape at all.
//!
//! So Houston writes one. Opinionated on purpose: "point Houston at your
//! existing Obsidian vault" produces a different layout for every user, and
//! nothing downstream — not the agent instructions, not the task view — can
//! assume anything about it.
//!
//! Every file here is ordinary markdown. Delete any of it and nothing breaks
//! except the convention it was describing.

/// The note that greets somebody opening the vault for the first time.
///
/// Lives with the rest of the scaffold rather than beside the folder-creating
/// code, so an older vault gaining the structure gains this too.
const WELCOME: &str = r#"# Start here

This vault is your knowledge base, and it is also where your agents should
start. Press `c` on the Vault view and Houston opens an agent right here, with
`AGENTS.md` already in its context.

That matters more than it sounds. An agent started in this folder knows that
`Projects/acme-api/index.md` says where the acme-api code actually lives, what
has been decided about it, and what happened last time somebody worked on it.
"Add a contact form to acme-web" becomes a sentence it can act on.

## What is here

- `AGENTS.md` — how agents should use this vault. Worth reading yourself
- `Projects/` — one folder per project. Copy `_template.md` for a new one
- `Tasks/` — one file per thing to do, and the Tasks view reads them
- `Plans/` — work thought through but not started
- `Knowledge/` — how you work, across every project
- `Daily/`, `Inbox/`, `Archive/` — if you want them

## Getting around

- `/` find a note by name, `f` search inside notes
- `↵` open, `e` edit, `n` new note, `N` new folder
- `y` copy a note's path, `i` send it into a running agent
- `c` start an agent in this vault

## The habit that makes it worth having

Tell your agents to write back. A project they have worked on should end up
with an `index.md` that reflects what they learned, a `build-log.md` entry when
something surprising happened, and a note in `decisions/` when something
non-obvious was settled. `CLAUDE.md` already asks them to; the rest is you
reminding them occasionally.
"#;

use anyhow::{Context, Result};
use std::path::Path;

/// The operating manual, in the file every agent looks for.
///
/// `AGENTS.md` rather than `CLAUDE.md` because it is an open standard rather
/// than one vendor's filename: donated to the Linux Foundation, and read by
/// Codex, Cursor, Gemini, Copilot, Amp, Zed and others. Houston does not care
/// which agent you run, so the instructions should not either.
const AGENTS_MD: &str = r"# Working in this vault

You are probably running *in this folder* rather than in a project. That is
deliberate. This vault knows where the code lives, what has been decided, and
what happened last time. Start here, then go where the work is.

## At the start of a session

1. **Check today's daily note.** `Daily/YYYY-MM-DD.md`. If there is not one and
   the person is starting work, offer to make it, and read yesterday's for
   anything left open.
2. **Read the task**, if there is one in `Tasks/` for this work.
3. **Read the project**: `Projects/<name>/index.md` for where the code lives,
   then its `decisions/` for what has already been argued.

Do not skip the third. Proposing something that was rejected two months ago
wastes an afternoon, and the reason it was rejected is written down.

## When asked what to work on

Read `Tasks/`. Suggest from `open` — those are ready to pick up — highest
priority first, and say which project each belongs to.

Mention `backlog` items as parked rather than as candidates. That status is a
decision to wait, usually because something has to happen first, and quietly
starting one is not initiative. Ask.

Ignore `done` and `cancelled` unless somebody asks what happened.

## Finding the code

Every folder in `Projects/` has an `index.md`, and its first job is to say
**where that project lives on disk**. So `add a contact form to acme-web` is a
complete instruction: read `Projects/acme-web/index.md`, take the path, work
there.

## When you finish

Write back. This is what makes the vault worth having, and the part that gets
skipped.

| what happened | where it goes |
|---|---|
| anything at all | a line in today `Daily/` note |
| work you found but did not do | a new file in `Tasks/` |
| something the diff cannot explain | the project `build-log.md` |
| a non-obvious decision | a numbered file in the project `decisions/` |
| a fact you had to work out | the project `knowledge/` |
| a measurement | the project `research/`, dated |
| how this person likes to work | root `Knowledge/` |

**Only write an entry if it says something the diff cannot.** `Added the
contact form` is already in the commit and helps nobody. `The form submits
through the legacy endpoint because the new one drops the honeypot field` is
worth a paragraph.

## Where things go

- `Projects/` — one folder per project. The code is elsewhere; this is what is
  known about it
- `Tasks/` — one file per thing to do. Frontmatter carries status, priority,
  project and tags, and Houston has a view over it. Open one for a follow-up
  you noticed rather than mentioning it in passing. `Tasks/README.md` says
  what the four statuses mean; the short version is that `open` is fair game
  and `backlog` is a decision to wait
- `Plans/` — work thought through but not started. An approved plan usually
  becomes tasks
- `Daily/` — one note per day. What happened, what is next
- `Knowledge/` — how *this person* works: their standards, preferences and
  conventions, across every project. Facts about one project belong in that
  project own `knowledge/`
- `Inbox/` — anything you cannot place yet. Better here than nowhere
- `Archive/` — finished, kept because search does not care

## Frontmatter

Notes may carry it; nothing requires it.

    ---
    project: acme-api
    tags: [auth, postgres]
    ---

Tasks use a little more. See `Tasks/README.md`.

## Two habits worth holding

**Record the losing argument.** A decision that says only what was chosen
invites somebody to re-derive the alternative and propose it again.

**A load-bearing claim needs a number.** `This is slow` needs a benchmark.
`Nobody uses this` needs a count. Put them in `research/` and link them.
";

/// Claude Code reads `CLAUDE.md`, so point it at the real thing.
///
/// An `@` import and a sentence saying the same: the import is what Claude
/// Code understands, and the sentence keeps the file useful to anything that
/// reads it without knowing the syntax.
const CLAUDE_MD: &str = r"# Claude

The operating manual for this vault is `AGENTS.md`. Read it before doing
anything here.

@AGENTS.md
";

/// A template for a new project, and an example of what one looks like filled in.
const PROJECT_TEMPLATE: &str = r"# Project name

**Where the code lives:** `~/path/to/the/repository`

One or two sentences on what this is and who it is for.

## Where things are

| what | where |
|---|---|
| the interesting part | `src/...` |
| tests | `tests/`, and what they need to run |
| deploys | how, and from which branch |

## Things worth knowing

The parts that surprised somebody. Conventions that are not obvious from the
code. Anything you had to work out once and would rather nobody worked out
again.

## Folders

- `build-log.md` — what happened here, newest first
- `decisions/` — one numbered file per decision, including the losing argument
- `research/` — measurements, dated
";

const EXAMPLE_PROJECT: &str = r"# example-project

**Where the code lives:** `~/work/example-project`

Delete this folder once you have a real one. It is here so the shape is
obvious rather than described.

## Where things are

| what | where |
|---|---|
| the app | `src/` |
| tests | `tests/`, needs a local database |
| deploys | merge to `main` |

## Things worth knowing

Every project folder in this vault looks like this one: an `index.md` that says
where the code is, a `build-log.md` of what happened, `decisions/` for the
arguments, and `research/` for the numbers.
";

const EXAMPLE_BUILD_LOG: &str = r"# Build log

Newest first. An entry earns its place only if it says something the diff
cannot.

## 2026-09-11

Set the project up. Nothing surprising yet.
";

const EXAMPLE_DECISION: &str = r"# 0001: Why decisions live in files

**Accepted, 2026-09-11.**

## The argument that won

An agent can read a folder. It cannot read the conversation where you decided
something six weeks ago, and neither can you by then.

## The argument that lost

That this is overhead for a small project. True for about a month, and then
somebody reopens a question that was already settled.
";

/// The shape the task view reads. Also readable and editable as plain markdown.
const EXAMPLE_TASK: &str = r"---
status: open
priority: normal
project: example-project
tags: [setup]
---

# Replace the example project with a real one

Write an `index.md` for something you actually work on, and put the real path
to the code at the top of it.

Everything below the frontmatter is yours. Houston shows the first heading as
the title and the rest as the description, so write whatever helps whoever
picks this up — including an agent, which can be started straight from here.
";

const TASKS_README: &str = r"# Tasks

One file per thing to do. Houston's Tasks view reads this folder, and you can
equally edit these as notes: it is the same files either way, and neither side
owns them.

The frontmatter Houston understands:

```
---
status: open        # backlog, open, done, cancelled
priority: normal    # low, normal, high
project: acme-api   # a folder name under Projects/
tags: [auth, tests]
---
```

Everything after it is the description. The first `#` heading is the title, so
renaming a task is editing that line.

## What the statuses mean

This is the part worth agreeing on, because it is what lets somebody — or
something — work out what to pick up without having to ask.

| status | means |
|---|---|
| `open` | ready to pick up. Fair game |
| `backlog` | agreed, but deliberately not started yet |
| `done` | finished |
| `cancelled` | decided against. Kept, because the decision is information |

**`backlog` is not a weaker `open`.** It means somebody has already thought
about this and decided *not yet* — usually because something else has to
happen first. Starting one without saying so is the kind of helpfulness nobody
asked for.

**`cancelled` is not delete.** A task that says why it is not happening stops
the same idea coming back in three months, which is the argument the
`decisions/` folders make about everything else.

**Nothing here is required.** A file with no frontmatter is an open task at
normal priority. A priority Houston does not recognise is normal. A status it
does not recognise stays open, so a task you marked `blocked` is visible rather
than quietly gone. You cannot write a markdown file here that breaks the view.

Houston edits one frontmatter line at a time and leaves the rest of the file
alone, so keys it has never heard of survive. Add your own.

## In Houston

`3` opens the view. `space` marks something done, `p` cycles priority, `e`
edits the description, `n` writes a new one, and `c` starts an agent on the
task — in the project's own directory, with the task and the project write-up
already in hand.

## For agents

Write tasks here. `0007-short-slug.md`, frontmatter as above, and a sentence
saying what would count as done. A follow-up you noticed while doing something
else belongs in a file, not at the end of a message nobody rereads.
";

const GLOBAL_LOG: &str = r"# Work log

Across all projects, newest first. Project-specific detail belongs in that
project's own `build-log.md`; this is for the things that span them.
";

const PLANS_README: &str = r"# Plans

Work thought through but not started. A plan is what you write before
committing an afternoon to something, and what you hand an agent when you want
it built rather than explored.

An approved plan usually turns into files in `Tasks/`. Keep it either way: it
says why those tasks look like they do.
";

const KNOWLEDGE_README: &str = r"# Knowledge

**How you work**, rather than what you are working on. Standards you hold,
tools you prefer, conventions you apply everywhere, the review checklist you
keep in your head. An agent that reads this should end up working the way you
would.

Facts about a particular project go in that project own `knowledge/` folder
instead. The test: if it stops being true when the project is archived, it is
project knowledge.
";

/// Whether this folder has already been given the structure.
///
/// `AGENTS.md` is the marker because it is the one file the whole convention
/// depends on: everything else is described *by* it.
pub fn is_scaffolded(root: &Path) -> bool {
    root.join("AGENTS.md").is_file()
}

/// Writes the starter vault, skipping anything already there.
///
/// Called on a vault Houston just created, and on an older one that predates
/// the structure — Houston 0.1 made a folder with a welcome note in it, and
/// those vaults would otherwise never gain any of this. Existing files are
/// never touched, so the backfill cannot overwrite something you wrote.
///
/// It only runs while [`is_scaffolded`] is false, which is what stops a file
/// you deliberately deleted from reappearing on every launch.
pub fn write(root: &Path) -> Result<()> {
    if is_scaffolded(root) {
        return Ok(());
    }

    let file = |path: &str, body: &str| -> Result<()> {
        let full = root.join(path);
        if full.exists() {
            return Ok(());
        }
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("could not create {}", parent.display()))?;
        }
        std::fs::write(&full, body).with_context(|| format!("could not write {}", full.display()))
    };

    file("Welcome.md", WELCOME)?;
    file("CLAUDE.md", CLAUDE_MD)?;
    file("log.md", GLOBAL_LOG)?;

    file("Projects/_template.md", PROJECT_TEMPLATE)?;
    file("Projects/example-project/index.md", EXAMPLE_PROJECT)?;
    file("Projects/example-project/build-log.md", EXAMPLE_BUILD_LOG)?;
    file(
        "Projects/example-project/decisions/0001-why-decisions-live-in-files.md",
        EXAMPLE_DECISION,
    )?;

    file("Tasks/README.md", TASKS_README)?;
    file("Tasks/0001-replace-the-example-project.md", EXAMPLE_TASK)?;

    file("Knowledge/README.md", KNOWLEDGE_README)?;
    file("Plans/README.md", PLANS_README)?;

    // Empty, but present: a folder that exists is an invitation, and a folder
    // that has to be created first is a decision nobody makes at the moment
    // they want to write something down.
    for empty in [
        "Projects/example-project/research",
        "Projects/example-project/knowledge",
        "Daily",
        "Inbox",
        "Archive",
    ] {
        std::fs::create_dir_all(root.join(empty))
            .with_context(|| format!("could not create {empty}"))?;
    }

    // Last, because it is the marker `is_scaffolded` reads. Written first, a
    // failure halfway through would leave a half-built vault that never gets
    // finished.
    file("AGENTS.md", AGENTS_MD)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("houston-scaffold-{name}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    /// Houston 0.1 created a vault with one welcome note in it. Those vaults
    /// exist on real machines and would otherwise never gain the structure.
    #[test]
    fn an_older_vault_gains_the_structure_without_losing_what_is_in_it() {
        let root = scratch("backfill");
        std::fs::write(root.join("Welcome.md"), "mine").unwrap();
        std::fs::create_dir_all(root.join("Projects/acme")).unwrap();
        std::fs::write(root.join("Projects/acme/index.md"), "real project").unwrap();

        assert!(!is_scaffolded(&root), "a bare folder has not been given the structure");
        write(&root).unwrap();

        assert_eq!(std::fs::read_to_string(root.join("Welcome.md")).unwrap(), "mine");
        assert_eq!(
            std::fs::read_to_string(root.join("Projects/acme/index.md")).unwrap(),
            "real project",
            "a real project must survive the backfill untouched"
        );
        assert!(root.join("Tasks/README.md").is_file(), "and the missing parts arrive");
        assert!(is_scaffolded(&root), "which is then recorded, so it happens once");

        std::fs::remove_dir_all(&root).ok();
    }

    /// Otherwise deleting the example project would undelete it every launch.
    #[test]
    fn a_file_you_deleted_stays_deleted() {
        let root = scratch("deleted");
        write(&root).unwrap();
        std::fs::remove_dir_all(root.join("Projects/example-project")).unwrap();

        write(&root).unwrap();

        assert!(
            !root.join("Projects/example-project").exists(),
            "the backfill runs once; after that the vault is yours"
        );

        std::fs::remove_dir_all(&root).ok();
    }

    /// The note that greets a new user has one job: say that agents should
    /// start in this folder, and why that makes them useful.
    #[test]
    fn a_new_vault_greets_whoever_opens_it() {
        let root = scratch("welcome");
        write(&root).unwrap();

        let welcome = std::fs::read_to_string(root.join("Welcome.md")).unwrap();
        assert!(welcome.contains("Start here"));
        assert!(welcome.contains("`c`"), "it names the key that starts an agent here");
        assert!(welcome.contains("AGENTS.md"), "and what the agent reads when it does");
        assert!(welcome.contains("Projects/"), "and how it finds the code from here");

        std::fs::remove_dir_all(&root).ok();
    }

    /// The file every agent reads. Without it the vault is just a folder.
    #[test]
    fn a_new_vault_tells_agents_how_to_use_it() {
        let root = scratch("agents");
        write(&root).unwrap();

        let guide = std::fs::read_to_string(root.join("AGENTS.md")).unwrap();
        assert!(guide.contains("where that project lives on disk"), "the point of Projects/");
        assert!(guide.contains("daily note"), "check the day before starting");
        assert!(guide.contains("build-log.md"), "and what to write when finishing");
        assert!(guide.contains("decisions/"), "and what to read before starting");

        std::fs::remove_dir_all(&root).ok();
    }

    /// Houston does not care which agent you run, so the instructions must not
    /// live in one vendor's filename. AGENTS.md is the open standard; CLAUDE.md
    /// exists only to send Claude Code to it.
    #[test]
    fn the_instructions_are_not_locked_to_one_agent() {
        let root = scratch("crossagent");
        write(&root).unwrap();

        let claude = std::fs::read_to_string(root.join("CLAUDE.md")).unwrap();
        assert!(claude.contains("@AGENTS.md"), "Claude Code imports it");
        assert!(claude.contains("AGENTS.md"), "and is told about it in prose too");
        assert!(
            claude.len() < 400,
            "CLAUDE.md is a pointer; two copies of the manual would drift apart"
        );

        std::fs::remove_dir_all(&root).ok();
    }

    /// Knowledge about how somebody works and knowledge about a project are
    /// different things, and putting them in one folder loses both.
    #[test]
    fn knowledge_is_split_between_the_person_and_the_project() {
        let root = scratch("knowledge");
        write(&root).unwrap();

        let general = std::fs::read_to_string(root.join("Knowledge/README.md")).unwrap();
        assert!(general.contains("How you work"), "the root one is about the person");
        assert!(
            root.join("Projects/example-project/knowledge").is_dir(),
            "and each project carries its own"
        );

        std::fs::remove_dir_all(&root).ok();
    }

    /// A shape described in prose is a shape nobody follows. The example
    /// project is there to be copied.
    #[test]
    fn the_example_project_shows_the_shape_rather_than_describing_it() {
        let root = scratch("shape");
        write(&root).unwrap();

        let project = root.join("Projects/example-project");
        assert!(project.join("index.md").is_file());
        assert!(project.join("build-log.md").is_file());
        assert!(project.join("decisions").is_dir());
        assert!(project.join("research").is_dir());

        let index = std::fs::read_to_string(project.join("index.md")).unwrap();
        assert!(
            index.contains("Where the code lives"),
            "an index that does not say where the code is misses the entire point"
        );

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn tasks_ship_with_an_example_of_the_frontmatter() {
        let root = scratch("tasks");
        write(&root).unwrap();

        let task = std::fs::read_to_string(root.join("Tasks/0001-replace-the-example-project.md"))
            .unwrap();
        for field in ["status:", "priority:", "project:", "tags:"] {
            assert!(task.contains(field), "the example task shows {field}");
        }

        std::fs::remove_dir_all(&root).ok();
    }

    /// Every file has to be a note, or the vault view shows a structure with
    /// holes in it.
    #[test]
    fn everything_written_is_markdown_the_vault_can_index() {
        let root = scratch("markdown");
        write(&root).unwrap();

        let files: Vec<_> = walkdir::WalkDir::new(&root)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
            .collect();

        assert!(files.len() >= 10, "a starter vault with almost nothing in it is an empty state");
        for entry in files {
            assert_eq!(
                entry.path().extension().and_then(|e| e.to_str()),
                Some("md"),
                "{} is not markdown",
                entry.path().display()
            );
        }

        std::fs::remove_dir_all(&root).ok();
    }
}
