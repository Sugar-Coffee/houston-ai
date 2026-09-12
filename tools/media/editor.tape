# Jump mode: press f, every word grows a tag, type it to go there.

Output media/editor.gif

Set Shell "bash"
Set FontSize 16
Set Width 1500
Set Height 820
Set Padding 20
Set Theme "Catppuccin Mocha"
Set Framerate 24
Set TypingSpeed 70ms

Hide
Type `eval "$(sh tools/media/fixture.sh)"`
Enter
Type `export HOUSTON_STATE_DIR=$STATE SHELL=/bin/sh PATH="$PWD/target/release:$PATH" && cd "$REPO" && clear`
Enter
Type "houston"
Enter
Sleep 3s
Type "2"
Sleep 1500ms
Down
Down
Down
Down
Enter
Sleep 900ms
Down
Enter
Sleep 900ms
Down
Down
Enter
Sleep 2s
Type "e"
Sleep 2500ms
Show

# Tags appear over the words, not between them.
Sleep 700ms
Type "f"
Sleep 2400ms
Escape
Sleep 600ms
Type "]"
Sleep 900ms
Type "t"
Sleep 1800ms
