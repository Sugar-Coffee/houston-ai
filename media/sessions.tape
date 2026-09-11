# Two real Claude Code agents, a shell, the diff, and the board.
#
#   sh media/fixture.sh    builds the workspace this records
#   vhs media/sessions.tape
#
# The agents are real, so the timings below are guesses with slack in them.
# If a GIF catches one mid-thought, lengthen the Sleep rather than trimming it:
# a recording that cuts an agent off is worse than one that waits.

Output media/sessions.gif

Set Shell "bash"
Set FontSize 15
Set Width 1600
Set Height 900
Set Padding 20
Set Margin 50
Set MarginFill "#181825"
Set BorderRadius 12
Set WindowBar "Colorful"
Set WindowBarSize 42
Set Theme "Catppuccin Mocha"
Set Framerate 24
Set TypingSpeed 45ms

Hide
Type `eval "$(sh media/fixture.sh)"`
Enter
Type `export HOUSTON_STATE_DIR=$STATE PATH="$PWD/target/release:$PATH" && cd "$REPO" && clear`
Enter
Show

Type "houston"
Enter
Sleep 3s

# First agent. `n` asks where and whether to isolate it.
Type "n"
Sleep 1500ms
Enter
Type "auth-refresh"
Enter
Sleep 800ms
Down
Down
Down
Enter
Sleep 7s

# Claude Code asks whether it trusts a folder the first time it sees one.
# Without this, everything typed below lands in that prompt and the agent
# never answers anything. "Yes" is already selected.
Enter
Sleep 5s

# Claude Code opens on a splash that greets you by name and names your
# organisation. `/clear` redraws it rather than removing it, so the way to get
# it off screen is conversation: it scrolls away after two exchanges. Hence a
# warm-up question that is worth asking anyway.
# Ask each agent about what it can actually see. This one is in the
# repository, so it gets asked about the code; the vault agent below gets
# asked about the notes. Asking the repo agent about decision records it has
# no access to records an agent saying it cannot find them, which demonstrates
# the wrong thing.
Type "What does src/auth/refresh.rs do? One sentence."
Enter
Sleep 30s

Type "What is uncommitted right now? Just the filenames."
Enter
Sleep 30s
Ctrl+\
Sleep 1500ms

# Second agent, in the vault, so it has the CLAUDE.md to hand.
Type "2"
Sleep 1500ms
Type "c"
Sleep 8s

# Same prompt again: a different directory is a different question.
Enter
Sleep 5s

# This one is in the vault, so it has the CLAUDE.md and the notes.
Type "Give me one line from Macbeth, nothing else."
Enter
Sleep 26s
Ctrl+\
Sleep 1500ms

# A shell, for the third card.
Type "1"
Sleep 800ms
Type "s"
Sleep 2s
Type "cargo test --quiet"
Enter
Sleep 2s
Ctrl+\
Sleep 2s

# What an agent changed.
Type "k"
Sleep 800ms
Type "v"
Sleep 4s
Escape
Sleep 1s

# The board.
Type "3"
Sleep 4s
Type "1"
Sleep 2s
