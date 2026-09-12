# Keybindings

The footer always shows what the current context accepts, so this is a
reference rather than something to memorise.

## How keys are printed

A cap shows the binding **exactly as you type it**. `n` is `n`, `N` is
shift-n, `esc` is esc.

This was upper-cased for a while, on the reasoning that a cap is a picture of a
key and the key on your keyboard says `D` whether or not you hold shift. True,
and beside the point: case *is* the notation here. `d` and `D` are different
bindings, and so are `g`/`G` and `n`/`N`. Upper-casing meant promoting that
distinction to a `⇧` marker and then asking you to translate `⇧D` back into
"hold shift and press d" — a step added to the one thing a footer exists to
tell you.

The cap's *fill* is what makes a key read as a key. That was always the idea.

**Nothing destructive sits behind a lone capital.** That rule stays, and it is
about danger rather than typography: `D` used to force-remove a worktree with
uncommitted work in it, which is now a question instead. A capital *paired with
its lower-case twin* is fine and always was — `g`/`G`, `n`/`N`.

Keys are context-dependent by design: `q` quits from the Sessions list, is a
character while attached to an agent, and closes the editor. What decides is
`App::focus()` — see `architecture.md`.

## The mouse

| | does |
|---|---|
| wheel, attached to a session | scrolls that session's scrollback; any keypress returns to the live output |
| wheel, anywhere else | scrolls whatever you are looking at |

A child that has asked for mouse reporting itself — a TUI running inside a
session — receives the events instead, which is why the wheel behaves normally
inside `vim` or `htop`.

Capturing the mouse costs your terminal's own click-drag text selection. Most
terminals let you hold **Shift** to bypass reporting and select anyway; if
yours does not, turn it off in Settings.

**If the wheel does nothing**, press `ctrl-g` to open the input inspector and
scroll. It shows every event the terminal sends. If no `mouse` lines appear,
your terminal is not reporting them — in iTerm2 that is
*Settings → Profiles → Terminal → Enable mouse reporting*. The keyboard
bindings below work regardless, which is why they exist.

## Everywhere

| key | does |
|---|---|
| `tab` / `shift-tab` | next / previous view |
| `1` – `6` | jump to Sessions / Vault / Tasks / Board / Worktrees / Settings |
| `q` | quit — **press twice**. Any other key cancels |
| `ctrl-c` | quit immediately |
| `ctrl-g` | input inspector — shows what the terminal is actually sending |

## Sessions

| key | does |
|---|---|
| `n` | new agent session — opens a form for name, directory and worktree |
| `s` | new shell session, started immediately in the default directory |
| `j` / `k`, `↑` / `↓` | move the selection |
| `↵` | attach |
| `v` | review — the session's uncommitted diff |
| `r` | rename (edited in place; empty restores the default) |
| `x` | close |

`v` is for *review*; `d` would be the better mnemonic and has been half-page
scrolling since Phase 1. The card itself carries a `+42 −8`, refreshed when a
hook fires and on the selected session's own tick, so the number you are
looking at is live and the ones you are not cost nothing.

### Reading a diff

| key | does |
|---|---|
| `j` / `k` | line |
| `u` / `d`, `pgup` / `pgdn` | page |
| `g` / `G` | top / bottom |
| `esc` / `q` / `v` | close |

### Copying text out of a session

**Use the mouse.** Drag inside a session pane and it copies on release, the way
a terminal does:

| | |
|---|---|
| drag | select characters |
| double-click | select a word |
| triple-click | select a line |
| release | copies, and says how many lines |

This works whether or not you are attached, and it works *inside the pane* —
which is the part your terminal cannot do for you. Your terminal selects its
own rows, and a Houston row is the sidebar and the pane side by side, so
option-dragging drags in both. Only Houston knows where the pane ends.

An empty selection is silent: clicking to put the cursor somewhere must not
clear your clipboard. Typing drops the highlight.

If a child asks for mouse reporting itself — a full-screen program with its own
click handling — its drags stay its own, and the keyboard mode below is the way
in.

#### By keyboard

`c` on a session you are *not* attached to. (Attached, every key belongs to the
child — `ctrl-\` first. That is why pressing `c` at an agent prompt types a
`c`.) Worth having for a precise selection spanning screens, and over SSH where
the mouse may not reach you at all.

| key | does |
|---|---|
| `hjkl`, arrows | move the cursor |
| `w` / `b` / `e` | by word |
| `0` / `^` / `$` | start of line / first non-blank / end |
| `H` / `M` / `L` | top / middle / bottom of the screen |
| `g` / `G` | top / bottom of the scrollback |
| `v` or space | start selecting, or drop the selection |
| `y` or `↵` | copy it and leave |
| `esc` | leave without copying |

**Why this exists rather than just dragging with the mouse.** Houston asks the
terminal to report mouse events so the wheel can scroll a session's history,
and a terminal that is reporting the mouse hands drags to the application
instead of using them for its own selection. Nothing Houston does can give that
back — the terminal has already decided.

If you would rather your terminal did the selecting — to grab the sidebar and
the pane together, say — **turn off "Capture the mouse"** in Settings. Native
selection comes back everywhere, and you lose wheel scrolling and in-pane
selection. Holding **option** (iTerm2) or **shift** (most others) while
dragging does the same for one gesture, and has the same whole-row problem.

### Scrolling a session's output

Two ways, because the mouse one depends on your terminal reporting mouse events
and that can be switched off:

| key | does |
|---|---|
| `u` / `d`, `pgup` / `pgdn` | page through history (not attached) |
| `g` / `G`, `home` / `end` | oldest / newest (not attached) |
| `shift-pgup` / `shift-pgdn` | page through history **while attached** |
| wheel | scrolls, if your terminal reports mouse events |

Any keypress while attached returns you to the live output. The pane shows
`↑ N lines back` while you are not looking at the present.

### While attached

Every key goes to the child, including `q`, the digits and `ctrl-c`.

| key | does |
|---|---|
| `ctrl-\` | detach |
| `shift-pgup` / `shift-pgdn` | scroll without detaching |

## Vault

The sidebar is a folder tree. `↵` opens a note, or opens and closes a folder.

| key | does |
|---|---|
| `j` / `k` | move |
| `↵` | open a note, or open/close a folder |
| `→` / `←` | open a folder · close it, or step out to the one holding this |
| `c` | start an agent **in the vault**, and go to it |
| `n` · `N` | new note · new folder — the name is a path, so `projects/kickoff` works |
| `r` | rename, or **move** it by typing a different folder into the name |
| `x` | delete — asks first, and says how many notes a folder holds |
| `/` | find by name |
| `f` | search inside notes |
| `y` · `i` | copy the path · send it to a session |
| `e` | edit |
| `l` · `b` | links out · backlinks |
| `w` | wrap long lines |
| `backspace` | back to the previous note |

**Choosing a result ends the query.** The filter comes off, the tree comes
back, and the note you picked is revealed and selected — so the answer to
"where is X" includes where it lives, not just what it was called.

Links and backlinks are left alone by that: those are a list you traverse, and
folding it away after one selection would make following a chain impossible.

Find and search still show a **flat list**, deliberately. A tree is a worse
answer to "where is the note called X" — it hides the thing you searched for
behind a folder you then have to open.

`←` and `→` rather than `h` and `l` because `l` has meant links-out since the
vault existed, and an asymmetric `h`/`l` pair would be worse than neither.

A new note opens straight in the editor, because that is why you made it. A new
folder does not, because there is nothing in it to open.

### An agent that already knows the vault

`c` starts an agent in the vault root and takes you to it, named `vault`.

The reason it earns a key: a vault set up with its own `CLAUDE.md`, skills and
rules gives an agent launched inside it the shape of your knowledge base and
where things live, for free. Doing that from the Sessions view means typing the
vault path every time — the alt-tab this app exists to remove.

Pressing it again goes to the agent that is already there rather than starting
a second one. Two agents in one directory share a hooks file, so the second
silences the first and the board goes quietly stale.

`c` for *chat*. `a` would be the better mnemonic in an app that says "agent"
everywhere, and it has meant "back to all notes" since the vault existed — not
worth taking away for a nicer letter.

### New notes and folders

`n` and `N` rather than one key and a "folder?" toggle. You know which of the
two you want before you start typing a name, so it belongs in the key you press
rather than in a field you have to go and find. This is the same kind of pair
as `g`/`G`, which is why it is allowed to involve a capital — see
[how keys are printed](#how-keys-are-printed).

## Board

| key | does |
|---|---|
| `h` / `l`, `←` / `→` | previous / next column, skipping empty ones |
| `j` / `k`, `↑` / `↓` | move within a column |
| `↵` | open that session |

## Editor

### Normal mode

| key | does |
|---|---|
| `h` `j` `k` `l`, arrows | move — `j`/`k` walk *visual* rows, so wrapped lines behave |
| `0` / `$`, `home` / `end` | line start / end |
| `g` / `G` | buffer start / end |
| `ctrl-d` / `ctrl-u`, page keys | page by a screenful of visual rows |
| `i` / `a` / `A` / `o` | insert here / after / at line end / on a new line |
| `f` | **jump mode** — type a tag to teleport the cursor |
| `/` then `↵` | search |
| `n` | next match |
| `[` / `]` | previous / next heading |
| `↵` | follow the `[[wikilink]]` under the cursor |
| `t` | tick or untick a task |
| `x` | delete the character |
| `d` | delete the line |
| `u` / `ctrl-r` | undo / redo |
| `s` | save |
| `S` | save, overwriting an external change |
| `esc` / `q` | close — twice if there are unsaved changes |

### Insert mode

| key | does |
|---|---|
| `esc` | back to normal |
| `↵` | new line, continuing a list and ending it on an empty item |
| `tab` | two spaces — a literal tab in markdown is a rendering hazard |

### Jump and search modes

| key | does |
|---|---|
| type | narrow the tags, or build the query |
| `esc` | cancel |
| `↵` | (search) run it |

## Forms — Settings, and the new-session dialog

A form is a menu: move to a row, press Return to act on it. Nothing is a live
text box until you say so, which is why `q` in a path is a `q`.

| key | does |
|---|---|
| `j` / `k`, `↑` / `↓`, `tab` | move between rows |
| `↵` | edit a text row, flip a toggle, or press an action row |
| `space` | flip a toggle |
| `esc` | (new-session form) cancel |

While editing a row:

| key | does |
|---|---|
| `tab` | complete a directory, as far as the candidates agree |
| `↵` | done |
| `esc` | done |

Settings apply the moment a row is committed. There is no save button.

## Settings

| key | does |
|---|---|
| `j` / `k`, `↑` / `↓` | move between settings |
| `↵` | edit the selected setting |
| `tab` | switch view (settings are walked with the arrows) |

### Powerline separators

Off by default. Turn it on and the tab strip draws as flowing arrow-shaped
segments, branch names take the powerline branch glyph, and the diff on a
session card becomes a green-into-red segment pair.

**It needs a powerline-patched font**, which is a smaller ask than it sounds:
[powerline/fonts](https://github.com/powerline/fonts) has patched a couple of
dozen ordinary families, and plenty of people installed it years ago for a
shell prompt. A [Nerd Font](https://www.nerdfonts.com/) also works, being a
superset.

The row shows you three things at once:

- **the glyphs themselves**, so you can see whether your font draws them
- **what a scan of your installed fonts found** — either the family it found
  them in, or a warning that nothing on this machine has them
- **an Install row underneath**, which downloads one patched font

The scan can only ever prove absence. If nothing installed has the glyphs, the
terminal certainly is not drawing them. If something does, your terminal may
still be pointed at a different font, which is why the glyphs are printed for
you to judge.

**Installing the font is not the last step.** Houston cannot change which font
your terminal uses — that lives in the terminal's own settings, in a different
place for each one. After installing, point your terminal at it:

| terminal | where |
|---|---|
| iTerm2 | Settings → Profiles → Text → Font |
| Terminal.app | Settings → Profiles → Text → Font → Change |
| Ghostty | `font-family` in `~/.config/ghostty/config` |
| WezTerm | `font = wezterm.font(...)` in `~/.wezterm.lua` |
| Alacritty | `[font.normal] family` in `alacritty.toml` |
| VS Code | `terminal.integrated.fontFamily` |

**Nerd Font icons were tried and removed.** Folder, note and shell icons live
in the private use area, and there is a nasty asymmetry there: ordinary Unicode
a font lacks falls back to another installed font and renders, but a private
use codepoint that *no* installed font claims has nothing to fall back to and
comes out as a question mark. Powerline separators are worth that risk because
they are one small, widely-patched block. Decorative icons were not.

## Tasks

`3`, or `tab` to it. One markdown file per task in the vault's `Tasks/` folder,
listed on the left, the selected one rendered on the right.

**A task is a card, not a row.** Title on its own line, then priority, status,
project and tags underneath. The one-line version gave the title whatever was
left after the metadata — about twenty characters — and the title is the only
part you read while scanning. Two lines and a gap costs a third of the visible
list and buys the whole title plus room to say things in words.

In the pane, those same four facts are a powerline stat bar when the setting is
on, and the same words separated by dots when it is not.

**`e` edits everything below the frontmatter.** The pane's border turns the
mode colour, the stat bar stays where it is, and you land in **normal mode at
the top**, exactly as if you had opened a note. It briefly opened in insert
mode on the theory that `e` means "write" — but this is the same editor with
the same modes everywhere else in Houston, and one that is modal in one place
and not in another is worse than either. `i`, `a`, `A`, `o` all mean what they
mean; so does `f` for jump mode and `/` for search.

Escape from normal mode finishes; it saves on the way out, and `s` saves
without leaving.

**The heading is in the buffer, because the heading is the title.** That is how
`Task::parse` finds it, so renaming a task is typing over its first line —
`esc`, `gg`, `A` — and the list updates as you type rather than waiting for the
save. No rename key, no second field, no dialog. Delete the heading entirely
and the title falls back to the filename, which is the same rule every other
malformed task here gets.

The *filename* does not follow a rename. `0003-old-name.md` stays
`0003-old-name.md`, because an agent may be holding that path and a
`[[wikilink]]` certainly might. The number is filing; the heading is the title.

### Inside the dialogs

`j`/`k` move between rows. What Return does depends on the row, and the bar
says which — `←→ change` on a row showing all its options, `↵ choose` on one
that opens a list, `↵ edit` on one you type into.

**Status and priority show every option, with the current one filled.** A
cycling field makes you press a key to find out what else it could be; three
options fit on a line, so they are all just there. The fill is the value's own
meaning colour — green for `open`, orange for `high` — rather than a generic
highlight, because the position already says "this one is selected" and the
colour can say something the position cannot.

**Tags are drawn as separate blocks.** `auth, security, api` in a text field is
a sentence you have to parse to see that it is three of something. All one
colour: a palette per tag would be a new hue for every word anybody invents,
and `ui::theme` has exactly as many hues as it has meanings.

### The two long lists

The project row opens a filtered list rather than cycling, for the reason the
theme row does: thirty projects cycled one keypress at a time is not a choice,
it is an endurance test. Type to narrow it, `↵` to take one.

Tags complete against **the tags already in your vault**, commonest first, with
suggestions appearing as you type rather than only on `Tab`. A vocabulary you
cannot see is a vocabulary nobody uses — the value of it is being reminded you
already have `auth` before you invent `authentication`. `Tab` takes the whole
suggestion and starts the next tag.

The frontmatter is the half the editor cannot reach: not on screen, not in the
buffer, so it cannot be typed into by accident, and the save splices the body
back in around it. If the file changed underneath — Obsidian, or an agent
writing to the same task — it asks before overwriting rather than winning the
race quietly.

| key | does |
|---|---|
| `j` / `k` | move |
| `↵` | details: status, priority, project, tags |
| `e` | edit the body — description and title — in the pane |
| `space` | mark done, or open again |
| `p` | raise the priority: low → normal → high, then round |
| `c` | start an agent on it, in that project's directory |
| `n` | new task |
| `x` | delete — asks first |
| `s` | sort: priority → newest → oldest |
| `a` | include the finished ones — done *and* cancelled |
| `r` | reread the folder |

### Four statuses

| status | means |
|---|---|
| `open` | ready to pick up |
| `backlog` | agreed, but deliberately not started yet |
| `done` | finished |
| `cancelled` | decided against, and kept because the decision is information |

The list shows `open` first, then `backlog`; `a` brings in **both** finished
ones — done and cancelled travel together, because the question the filter
asks is "is this still on my list", and neither of them is. The count in the
header is open tasks only, because that is the number the view exists to
answer.

`s` changes what the list is sorted by *within* each status: priority (the
default), newest, oldest. Status stays the primary key whatever you pick, so
switching sorts never lifts a finished task above a live one. The current
sort and filter are both written along the bottom edge of the list, because a
mode you cannot see is a mode you will be surprised by.

**There is no `created:` field, and there does not need to be.** The `0007` in
a task's filename is handed out in order, so it already *is* the date it was
added — on every task ever written, including ones an agent typed straight
into the folder. A date field would be a second fact to keep true, absent from
half the files, and wrong the moment somebody copies one. A task with no number
sorts as the newest thing there is, because something just put it there.

**`backlog` is not a weaker `open`, and it is not hidden either.** It is work
somebody has already decided to defer, so it stays visible — dimming it in with
the finished ones would file it with the things nobody is going to look at
again. It takes ordinary text where `open` takes green: present, not lit.

**The vocabulary is for the agents as much as for you.** `Tasks/README.md` and
`AGENTS.md` in the starter vault both spell it out, so an agent asked what to
work on can suggest from `open`, mention `backlog` as parked, and not quietly
start something that was deferred on purpose.

**A task has two halves and they want different tools.** The body is prose, so
`e` gives it the editor. The metadata is four values from fixed vocabularies,
so `↵` gives it a dialog — because free text there is not freedom, it is the
opportunity to type `hihg` and have the task quietly sort as normal.

The details dialog writes one frontmatter key at a time, so keys it never
showed you survive a trip through it.

**Nothing you type into these files can break the view.** No frontmatter is an
open task at normal priority. An unrecognised priority is normal. An
unrecognised status stays *open*, so a task you marked `blocked` is still
visible rather than quietly gone. No heading means the title comes from the
filename.

**And Houston will not rewrite them.** Changing a priority replaces one line
and leaves every other byte alone, so keys it has never heard of survive
untouched. That is the direction the risk actually runs: a person hand-editing
markdown cannot break a parser that never fails, but an app that owns the file
normalises away whatever it does not understand.

`c` is the one worth knowing. The task names a project, the project's
`index.md` says where the code lives, and the agent opens *there* already
holding the task, the project write-up and the decisions behind it. If the
project has no recorded path the agent starts in the vault instead, which is
still enough for it to find its own way. Claude Code and Codex take the
briefing as an opening prompt; an agent with no checked way of accepting one
starts plain and Houston says so.

## Worktrees

A view of its own (`5`, or `tab` to it), not a popup. It was a modal box
until a worktree started carrying a branch, a tracking count, a diff, a status
and a path, at which point there was nowhere to put five facts about ten
worktrees inside something floating over something else.

| key | does |
|---|---|
| `j` / `k` | move |
| `↵` | go to the session working in it |
| `v` | review its uncommitted work |
| `l` | land — commit, push, open a pull request, remove |
| `d` | remove — asks first if there is uncommitted work in it |
| `r` | refresh |

Each row carries a status, which is the question the view exists to answer:

| | means | removing it |
|---|---|---|
| `● session running` | an agent or shell is working in it | close its session first |
| `◆ no session, uncommitted` | nothing is using it, and there is work in it you have not committed | loses that work |
| `· no session, clean` | nothing is using it, and nothing would be lost | safe |
| `⚠ repository gone` | its repository has been moved or deleted | git cannot help; it just deletes the directory |

**Each label names the reason.** "idle" was true of three of those and told you
nothing about any of them. A worktree nobody is using and a worktree whose
repository has been deleted are both "not doing anything", and they want
completely different things from you.

The selected row also spells out what removing it would cost, since that is a
question about one worktree rather than all of them.

A worktree in use by a live session is never removed. Nor is one with
uncommitted work, without answering a question first.

## Questions

Anything that would lose work asks before doing it:

| key | does |
|---|---|
| `y` | yes |
| `n`, `esc`, and everything else | no |

**Anything that is not clearly yes is no**, Return included. Return is what you
press to dismiss things, so it must not be what confirms a deletion.

This replaced a pair of shift variants — `D` to remove a worktree with
uncommitted work, `S` to save over a note that changed on disk. Those had two
problems. You had to already know the capital existed, so the first time you
met the situation the app said no and stopped; and the safety of it rested on
your shift finger rather than on your having read what was at stake. A question
names what will be lost, and works the first time.

A **clean** worktree is removed without a question. It is a directory git can
recreate from the branch, and asking about it would only train you to answer
yes without reading.
Rows also show `↑ahead ↓behind` once there is something to say, and the diff in
the same colours it has everywhere else.

### Landing

`l` opens a form titled with the branch and where it is going, because pushing
and opening a pull request are visible to other people. Push and pull request
are on by default; **removing the worktree is not**, since it is the only step
here that cannot be undone. If the worktree is in use by a live session, the
removal toggle is not offered at all.

A pull request implies a push even if you untick it — a PR needs a branch the
remote can see, and failing at the PR is a poor way to learn that. The form
stays open if anything fails, and says how far it got, because the fix is
usually one toggle.

Opening a pull request needs the `gh` CLI. Without it, untick that row.

## Choosers

| key | does |
|---|---|
| `j` / `k` | move |
| `1` – `9` | pick directly, numbered as the sidebar is |
| `↵` | send |
| `esc` / `q` | cancel |
