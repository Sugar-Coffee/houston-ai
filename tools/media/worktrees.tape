# Worktrees, and what each one is for.

Output media/worktrees.gif

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
Type "4"
Sleep 2s
Show

# Each row says why it is in the state it is in, and the selected one says
# what removing it would cost.
Sleep 1200ms
Type "j"
Sleep 1800ms
Type "j"
Sleep 2200ms
Type "k"
Sleep 1500ms
