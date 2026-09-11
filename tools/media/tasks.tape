# The task list, and the fact that it is only markdown.

Output media/tasks.gif

Set Shell "bash"
Set FontSize 15
Set Width 1600
Set Height 900
Set Padding 20
Set Theme "Catppuccin Mocha"
Set Framerate 24
Set TypingSpeed 55ms

Hide
Type `eval "$(sh tools/media/fixture.sh)"`
Enter
Type `export HOUSTON_STATE_DIR=$STATE PATH="$PWD/target/release:$PATH" && cd "$REPO" && clear`
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
Sleep 1100ms

# And write a new one.
Type "n"
Sleep 400ms
Enter
Type "Cache the JWKS response"
Sleep 200ms
Enter
Sleep 500ms
Down
Enter
Sleep 600ms
Down
Down
Down
Enter
Sleep 1800ms
