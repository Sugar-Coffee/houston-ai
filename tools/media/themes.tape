# Nineteen themes, and the app repaints as you move through them.

Output media/themes.gif

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
Sleep 2500ms
Type "s"
Sleep 2s
Ctrl+\
Sleep 800ms
Type "6"
Sleep 1500ms
Down
Down
Enter
Sleep 1500ms
Show

# Every step repaints the whole thing, so you pick by looking.
Sleep 800ms
Type "j"
Sleep 900ms
Type "j"
Sleep 900ms
Type "j"
Sleep 900ms
Type "j"
Sleep 900ms
Type "j"
Sleep 1100ms
Type "k"
Sleep 900ms
Enter
Sleep 1800ms
