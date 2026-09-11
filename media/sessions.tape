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
Type `printf 'theme = "Catppuccin Mocha"\nvault = "%s"\n' "$VAULT" > "$STATE/config.toml"`
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
Sleep 6s

Type "What is the p99 auth latency after the refresh change? One sentence."
Enter
Sleep 20s
Ctrl+\
Sleep 1500ms

# Second agent, in the vault, so it has the CLAUDE.md to hand.
Type "2"
Sleep 1500ms
Type "c"
Sleep 8s
Type "Give me one line from Macbeth, nothing else."
Enter
Sleep 18s
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
