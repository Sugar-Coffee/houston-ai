# Jump mode: press f, every word head gets a tag, type it to go there.

Output media/editor.gif

Set Shell "bash"
Set FontSize 15
Set Width 1500
Set Height 820
Set Padding 18
Set Theme "Catppuccin Mocha"
Set TypingSpeed 60ms

Hide
Type `export HOUSTON_STATE_DIR=$(mktemp -d) VAULT=$(mktemp -d)`
Enter
Type `mkdir -p "$VAULT/Projects" && printf '# Kickoff\n\nThe API rewrite starts on Monday.\nAuthentication moves to the new service first.\n\n## Open questions\n\n- Which database wins\n- Who owns the migration\n' > "$VAULT/Projects/kickoff.md"`
Enter
Type `printf 'vault = "%s"\n' "$VAULT" > "$HOUSTON_STATE_DIR/config.toml"`
Enter
Type "clear"
Enter
Show

Type "./target/release/houston"
Enter
Sleep 1500ms

# Into the vault, open the folder, open the note.
Type "2"
Sleep 1200ms
Enter
Sleep 1200ms
Down
Enter
Sleep 2s

# Edit it, then jump.
Type "e"
Sleep 1500ms
Type "f"
Sleep 2500ms
Escape
Sleep 1s

# Headings, and a task.
Type "]"
Sleep 1200ms
Escape
Sleep 1500ms
