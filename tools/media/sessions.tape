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
Type `eval "$(sh tools/media/fixture.sh)"`
Enter
Type `export HOUSTON_STATE_DIR=$STATE SHELL=/bin/sh PATH="$PWD/target/release:$PATH" && cd "$REPO" && clear`
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

# A second question, and not because one was not enough. Claude Code opens on
# a splash carrying the account name and the email it is signed in with, and
# `/clear` redraws it rather than removing it — the only thing that takes it
# off screen is enough conversation to scroll it off. This is a public README;
# check a frame after changing anything above this line.
Type "Now list every file under src/ with a one-line description of each."
Enter
Sleep 40s
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

# Same reason as above: enough conversation to scroll the startup banner off.
# It carries the plan name and a warning about the recorder's own MCP servers,
# neither of which is anything to do with Houston.
Type "Now list the projects in this vault, one line each."
Enter
Sleep 40s
Ctrl+\
Sleep 1s

# And a shell.
Type "1"
Sleep 500ms
Type "s"
Sleep 2500ms
Type "git --no-pager status --short && ls src/auth"
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
Type "4"
Sleep 2400ms
