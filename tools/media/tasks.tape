# The task list, and the fact that it is only markdown.

Output media/tasks.gif

Set Shell "bash"
Set FontSize 16
Set Width 1600
Set Height 680
Set Padding 20
Set Theme "Catppuccin Mocha"
Set Framerate 24
Set TypingSpeed 55ms

Hide
Type `eval "$(sh tools/media/fixture.sh)"`
Enter
Type `export HOUSTON_STATE_DIR=$STATE SHELL=/bin/sh PATH="$PWD/target/release:$PATH" && cd "$REPO" && clear`
Enter
Type "houston"
Enter
Sleep 3s
Type "3"
Sleep 2s
Show

# Read down the list. The pane keeps up.
Sleep 600ms
Down
Sleep 800ms
Down
Sleep 800ms

# Tick one off.
Space
Sleep 900ms

# The description is edited in the pane, and the heading is the title —
# so renaming is typing over it, and the list keeps up.
Up
Sleep 400ms
Type "e"
Sleep 700ms
Type "A"
Sleep 300ms
Type " on use"
Sleep 1000ms
Escape
Sleep 300ms
Escape
Sleep 900ms

# One more, so the rename is visibly not a one-off.
Down
Sleep 1200ms
