# The vault as a folder tree, and starting an agent inside it.

Output media/vault.gif

Set Shell "bash"
Set FontSize 15
Set Width 1500
Set Height 820
Set Padding 18
Set Theme "Catppuccin Mocha"
Set TypingSpeed 55ms

Hide
Type `export HOUSTON_STATE_DIR=$(mktemp -d) VAULT=$(mktemp -d)`
Enter
Type `mkdir -p "$VAULT/Projects/acme-api/decisions" "$VAULT/Knowledge" "$VAULT/Daily"`
Enter
Type `printf '# How agents use this vault\n' > "$VAULT/CLAUDE.md"`
Enter
Type `printf '# acme-api\n' > "$VAULT/Projects/acme-api/index.md"`
Enter
Type `printf '# Postgres over Dynamo\n' > "$VAULT/Projects/acme-api/decisions/0001-postgres.md"`
Enter
Type `printf 'vault = "%s"\n' "$VAULT" > "$HOUSTON_STATE_DIR/config.toml"`
Enter
Type "clear"
Enter
Show

Type "./target/release/houston"
Enter
Sleep 1500ms

Type "2"
Sleep 1200ms

# Walk into the tree.
Enter
Sleep 1s
Down
Enter
Sleep 1s
Down
Enter
Sleep 1500ms

# A new note, in the folder you were looking at.
Type "n"
Sleep 800ms
Enter
Sleep 500ms
Type "0002-caching"
Sleep 800ms
Enter
Sleep 500ms
Down
Enter
Sleep 2s
Escape
Sleep 1500ms
