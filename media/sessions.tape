# Several agents at once, and the board.
#
# Run from the repository root, after `cargo build --release`.

Output media/sessions.gif

Set Shell "bash"
Set FontSize 15
Set Width 1500
Set Height 820
Set Padding 18
Set Theme "Catppuccin Mocha"
Set TypingSpeed 45ms

# A throwaway state directory, so this never touches a real ~/.houston.
Hide
Type `export HOUSTON_STATE_DIR=$(mktemp -d) && cd "$(mktemp -d)" && git init -q`
Enter
Type `printf 'one\ntwo\nthree\n' > notes.txt && git add -A && git commit -qm first`
Enter
Type "clear"
Enter
Show

Type "./target/release/houston"
Enter
Sleep 2s

# A shell, then a second one, so the sidebar has something to show.
Type "s"
Sleep 1500ms
Type "echo 'agents run here'"
Enter
Sleep 1s
Ctrl+\
Sleep 800ms

Type "s"
Sleep 1500ms
Type "printf 'four\nfive\n' >> notes.txt"
Enter
Sleep 1s
Ctrl+\
Sleep 1s

# The diff each session has produced.
Type "v"
Sleep 2500ms
Escape
Sleep 800ms

# The board.
Type "3"
Sleep 2500ms

# Worktrees.
Type "4"
Sleep 2s

Type "1"
Sleep 1500ms
