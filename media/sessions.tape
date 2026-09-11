# Several agents, one key to move between them.
#
# Everything up to `Show` is setup: starting Houston, starting two real Claude
# Code agents, asking them something, waiting for the answers. None of it is
# recorded. vhs executes hidden commands without capturing frames, which is the
# only way a README GIF stays under ten seconds while the thing it is showing
# takes three minutes to arrange.

Output media/sessions.gif

Set Shell "bash"
Set FontSize 15
Set Width 1600
Set Height 900
Set Padding 20
Set Theme "Catppuccin Mocha"
Set Framerate 24
Set TypingSpeed 45ms

Hide
Type `eval "$(sh media/fixture.sh)"`
Enter
Type `export HOUSTON_STATE_DIR=$STATE PATH="$PWD/target/release:$PATH" && cd "$REPO" && clear`
Enter

Type "houston"
Enter
Sleep 3s

# An agent in the repository.
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
Sleep 8s
Enter
Sleep 5s
Type "What does src/auth/refresh.rs do? One sentence."
Enter
Sleep 35s
Ctrl+\
Sleep 1s

# An agent in the vault, which has the CLAUDE.md and the notes.
Type "2"
Sleep 1500ms
Type "c"
Sleep 9s
Enter
Sleep 5s
Type "Which decision covers token refresh? Just the filename."
Enter
Sleep 35s
Ctrl+\
Sleep 1s

# And a shell.
Type "1"
Sleep 500ms
Type "s"
Sleep 2500ms
Type "cargo build --quiet"
Enter
Sleep 2s
Ctrl+\
Sleep 1s
Type "g"
Sleep 1s
Show

# Ten seconds of the actual thing: three sessions, one key between them.
Sleep 900ms
Type "j"
Sleep 1600ms
Type "j"
Sleep 1600ms
Type "k"
Sleep 1100ms
Type "3"
Sleep 2400ms
