# The board. Driven by agent hooks, so it changes while you watch.

Output media/board.gif

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
Type `export HOUSTON_STATE_DIR=$STATE PATH="$PWD/target/release:$PATH" && cd "$REPO" && clear`
Enter
Type "houston"
Enter
Sleep 3s

# An agent, and a shell, so the board has more than one column in it.
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
Sleep 30s
Ctrl+\
Sleep 800ms
Type "s"
Sleep 2500ms
Ctrl+\
Sleep 800ms

# Ask something slow, then watch the board rather than the agent.
Type "k"
Sleep 500ms
Enter
Sleep 1500ms
Type "Read every file under migrations/ and summarise each in one line."
Enter
Sleep 1200ms
Ctrl+\
Sleep 500ms
Type "3"
Sleep 500ms
Show

# It moves between columns on its own, because the state comes from hooks.
Sleep 7s
