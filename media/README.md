# Recording the GIFs

The README's images are generated, not hand-captured, so they can be redone
after a UI change without anyone having to remember what was in the last one.

```sh
brew install vhs          # or: go install github.com/charmbracelet/vhs@latest
cargo build --release
vhs media/sessions.tape
vhs media/editor.tape
vhs media/vault.tape
```

Then uncomment the `<img>` tags in `README.md`.

Each tape points `HOUSTON_STATE_DIR` at a throwaway directory. **Do not remove
that** — without it a recording writes into your real `~/.houston`, and a demo
session ends up in your actual session list.
