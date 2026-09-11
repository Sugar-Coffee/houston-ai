# The vault as a folder tree, and finding a note in it.

Output media/vault.gif

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
Type "2"
Sleep 2s
Show

# Walk into a project and open a decision.
Sleep 600ms
Down
Down
Down
Down
Enter
Sleep 600ms
Down
Enter
Sleep 600ms
Down
Enter
Sleep 700ms
Down
Enter
Sleep 1800ms

# Find by name. Picking a result drops the filter and shows where it lives.
Type "/"
Sleep 300ms
Type "latency"
Sleep 1100ms
Enter
Sleep 300ms
Enter
Sleep 2200ms
