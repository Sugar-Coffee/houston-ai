#!/bin/sh
# Records all three GIFs. Run from the repository root, in a normal terminal.
#
#   sh media/record.sh            everything
#   sh media/record.sh vault      just one
#
# Must be a terminal you are actually sitting in front of. vhs drives a
# headless Chrome, and Chrome needs a macOS window server session to render
# anything. Run from something without one and vhs reports success, captures
# zero frames, and writes nothing.

set -eu

command -v vhs >/dev/null || { echo "vhs not found. brew install vhs" >&2; exit 1; }

echo "Building the release binary..."
cargo build --release

TAPES="${*:-vault editor sessions}"

for name in $TAPES; do
    tape="media/${name}.tape"
    [ -f "$tape" ] || { echo "no such tape: $tape" >&2; exit 1; }

    echo
    echo "── $name ──────────────────────────────────────────"
    [ "$name" = "sessions" ] && echo "  Real Claude Code agents. Takes a couple of minutes."

    vhs "$tape"

    if [ -s "media/${name}.gif" ]; then
        printf '  ✓ media/%s.gif  (%s)\n' "$name" "$(du -h "media/${name}.gif" | cut -f1)"
    else
        echo "  ✗ nothing was written." >&2
        echo "    If this is not a terminal you are sitting in front of, that is why." >&2
        exit 1
    fi
done

echo
echo "Done. Watch them, then uncomment the <img> tags in README.md."
echo "Especially sessions.gif: whatever the agents said is now in your README."
