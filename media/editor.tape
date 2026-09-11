# Jump mode: press f, every word grows a tag, type it to go there.
#
#   sh media/fixture.sh && vhs media/editor.tape

Output media/editor.gif

Set Shell "bash"
Set FontSize 16
Set Width 1500
Set Height 820
Set Padding 20
Set Margin 50
Set MarginFill "#181825"
Set BorderRadius 12
Set WindowBar "Colorful"
Set WindowBarSize 42
Set Theme "Catppuccin Mocha"
Set Framerate 24
Set TypingSpeed 70ms

Hide
Type `eval "$(sh media/fixture.sh)"`
Enter
Type `export HOUSTON_STATE_DIR=$STATE PATH="$PWD/target/release:$PATH" && cd "$REPO" && clear`
Enter
Show

Type "houston"
Enter
Sleep 2s

# Open the longest note in the vault and edit it.
Type "2"
Sleep 1500ms
Down
Down
Down
Down
Enter
Sleep 1200ms
Down
Enter
Sleep 1200ms
Down
Down
Enter
Sleep 2500ms
Type "e"
Sleep 2500ms

# Jump mode. Tags overlay the words rather than pushing them around.
Type "f"
Sleep 3500ms
Escape
Sleep 1500ms

# Headings, and a task toggle.
Type "]"
Sleep 1200ms
Type "]"
Sleep 1500ms
Type "o"
Sleep 800ms
Type "- [ ] measure the single-flight version under the same soak"
Sleep 1500ms
Escape
Sleep 800ms
Type "t"
Sleep 2500ms
Escape
Sleep 2s
