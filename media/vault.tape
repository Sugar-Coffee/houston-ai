# The vault: a folder tree, a note, find, and an agent that knows the place.
#
#   sh media/fixture.sh && vhs media/vault.tape

Output media/vault.gif

Set Shell "bash"
Set FontSize 15
Set Width 1600
Set Height 900
Set Padding 20
Set Margin 50
Set MarginFill "#181825"
Set BorderRadius 12
Set WindowBar "Colorful"
Set WindowBarSize 42
Set Theme "Catppuccin Mocha"
Set Framerate 24
Set TypingSpeed 45ms

Hide
Type `eval "$(sh media/fixture.sh)"`
Enter
Type `printf 'theme = "Catppuccin Mocha"\nvault = "%s"\n' "$VAULT" > "$STATE/config.toml"`
Enter
Type `export HOUSTON_STATE_DIR=$STATE PATH="$PWD/target/release:$PATH" && cd "$REPO" && clear`
Enter
Show

Type "houston"
Enter
Sleep 2s
Type "2"
Sleep 2s

# Walk into a project.
Down
Down
Down
Down
Enter
Sleep 1500ms
Down
Enter
Sleep 1500ms
Down
Enter
Sleep 2500ms

# Read a decision.
Down
Enter
Sleep 4s

# Find by name. Picking a result drops the filter and shows where it lives.
Type "/"
Sleep 800ms
Type "latency"
Sleep 2s
Enter
Sleep 800ms
Enter
Sleep 4s

# A new note lands beside what you were looking at.
Type "n"
Sleep 1s
Enter
Sleep 600ms
Type "0003-single-flight"
Sleep 1s
Enter
Sleep 600ms
Down
Enter
Sleep 3s
Escape
Sleep 2s
