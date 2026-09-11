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

use anyhow::{Context, Result};
use std::path::Path;

/// The operating manual. An agent started in the vault reads this first.
const CLAUDE_MD: &str = r#"# How agents work in this vault

This vault is the home base. You were probably started here rather than in a
project, and that is deliberate: this folder knows where the projects are and
what has already been decided about them.

## Finding a project

`Projects/` holds one folder per project. Each has an `index.md` whose first
job is to say **where the code actually lives on disk**. Read it before you go
anywhere.

So "add a contact form to acme-web" means: read `Projects/acme-web/index.md`,
find the path, work there.

## Before you start

Read the project's `index.md`, and everything in its `decisions/`. Most large
questions have been argued already, and the argument that lost is recorded
next to the one that won. Proposing a rewrite that was rejected in March
wastes everybody's afternoon.

If there is a `Tasks/` item pointing at this work, read that too.

## When you finish

Append to the project's `build-log.md`, newest first, dated absolutely
(`2026-09-11`, not "today"). **Write an entry only if it says something the
diff cannot**: a dead end, a measurement that settled an argument, a library
that behaved unexpectedly. "Added the contact form" is already in the commit.

If you decided something non-obvious, add a numbered file to `decisions/`.
Record the argument that lost as well as the one that won.

If you measured something, put the numbers in `research/`. Any load-bearing
claim needs one: "this is slow" needs a benchmark, "nobody uses this" needs a
count.

## Keep this vault up to date

You are expected to write here, not just read. A project you have worked on
and learned something about should have an `index.md` that reflects what you
now know. If you had to work something out that was not written down, write it
down.
"#;

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
equally edit these as notes: it is the same files either way.

The frontmatter Houston understands:

```
---
status: open        # or done
priority: high      # high, normal, low
project: acme-api   # a folder name under Projects/
tags: [auth, tests]
---
```

Everything after it is the description. The first `#` heading is the title.

Nothing here is required. A file with no frontmatter is an open task with no
project and no tags.
";

const GLOBAL_LOG: &str = r"# Work log

Across all projects, newest first. Project-specific detail belongs in that
project's own `build-log.md`; this is for the things that span them.
";

const KNOWLEDGE_README: &str = r"# Knowledge

Reference that outlives any one project. How the infrastructure actually
works, who owns what, the thing you look up every six months and can never
find.

If a note here only makes sense inside one project, it belongs in that
project's folder instead.
";

/// Writes the starter vault into an empty directory.
///
/// Only ever called on a directory Houston just created, so nothing here
/// checks for existing files.
pub fn write(root: &Path) -> Result<()> {
    let file = |path: &str, body: &str| -> Result<()> {
        let full = root.join(path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("could not create {}", parent.display()))?;
        }
        std::fs::write(&full, body).with_context(|| format!("could not write {}", full.display()))
    };

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

    // Empty, but present: a folder that exists is an invitation, and a folder
    // that has to be created first is a decision nobody makes at the moment
    // they want to write something down.
    for empty in ["Projects/example-project/research", "Daily", "Archive"] {
        std::fs::create_dir_all(root.join(empty))
            .with_context(|| format!("could not create {empty}"))?;
    }

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

    /// The file an agent reads first. Without it the vault is just a folder.
    #[test]
    fn a_new_vault_tells_agents_how_to_use_it() {
        let root = scratch("claude");
        write(&root).unwrap();

        let guide = std::fs::read_to_string(root.join("CLAUDE.md")).unwrap();
        assert!(guide.contains("where the code actually lives"), "the point of Projects/");
        assert!(guide.contains("build-log.md"), "and what to write when finishing");
        assert!(guide.contains("decisions/"), "and what to read before starting");

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

        assert!(files.len() >= 8, "a starter vault with almost nothing in it is an empty state");
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
