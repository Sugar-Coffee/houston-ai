#!/bin/sh
# Records the README GIFs. Run from the repository root.
#
#   sh media/record.sh            all of them
#   sh media/record.sh vault      just one
#
# vhs captures the frames; ffmpeg does the encoding here rather than letting
# vhs do it. See "Why the encode is ours" in media/README.md.

set -eu

command -v vhs    >/dev/null || { echo "vhs not found. brew install vhs" >&2; exit 1; }
command -v ffmpeg >/dev/null || { echo "ffmpeg not found. brew install ffmpeg" >&2; exit 1; }

# Catppuccin Mocha, which every tape sets. `base` behind the terminal, `crust`
# behind everything.
TERMINAL_BG="#1E1E2E"
BACKDROP="#11111B"
PADDING=22
MARGIN=40
FPS=13
TARGET_WIDTH=1040
# A terminal is flat colour and large areas of it do not change between frames.
# 128 is more than enough for that and roughly halves the file, which matters
# for something a README loads on sight.
COLOURS=128

# Playback speed per tape. The sessions recording is mostly time spent waiting
# for an agent to answer, which is honest and extremely boring: five minutes of
# real time is not a README image. The others run at life speed because they
# are already short.
speed_for() {
    case "$1" in
        sessions) echo 2.6 ;;
        *)        echo 1 ;;
    esac
}

echo "Building the release binary..."
cargo build --release

for name in ${*:-vault editor sessions}; do
    tape="media/${name}.tape"
    [ -f "$tape" ] || { echo "no such tape: $tape" >&2; exit 1; }

    echo
    echo "── $name ─────────────────────────────────────────────"
    [ "$name" = "sessions" ] && echo "   Real Claude Code agents. A couple of minutes."

    frames="$(mktemp -d)/frames"
    work="$(mktemp -d)/capture.tape"

    # Same tape, pointed at a frame directory. vhs creates it, so it must not
    # exist yet: given a directory that is already there it writes nothing.
    sed "s|^Output .*|Output \"${frames}/\"|" "$tape" > "$work"

    echo "   Capturing..."
    vhs "$work" >/dev/null 2>&1 || true

    count=$(find "$frames" -name 'frame-text-*.png' 2>/dev/null | wc -l | tr -d ' ')
    if [ "$count" -lt 10 ]; then
        echo "   ✗ only $count frames captured." >&2
        echo "     Run this from a terminal you are sitting in front of." >&2
        exit 1
    fi
    echo "   $count frames."

    size=$(ffprobe -v error -select_streams v:0 -show_entries stream=width,height \
        -of csv=p=0:s=x "$frames/frame-text-00001.png")

    speed=$(speed_for "$name")
    echo "   Encoding at ${speed}x..." 
    # The text layer is glyphs on transparency, so it needs a background under
    # it before anything else. Compositing straight onto the backdrop puts the
    # backdrop colour between every letter.
    ffmpeg -hide_banner -loglevel error -y \
        -f lavfi -i "color=${TERMINAL_BG}:size=${size}:rate=${FPS}" \
        -framerate "$FPS" -i "$frames/frame-text-%05d.png" \
        -framerate "$FPS" -i "$frames/frame-cursor-%05d.png" \
        -filter_complex "\
[0][1]overlay=shortest=1[t];\
[t][2]overlay=shortest=1,\
setpts=PTS/${speed},\
pad=iw+$((PADDING*2)):ih+$((PADDING*2)):${PADDING}:${PADDING}:color=${TERMINAL_BG},\
pad=iw+$((MARGIN*2)):ih+$((MARGIN*2)):${MARGIN}:${MARGIN}:color=${BACKDROP},\
scale=${TARGET_WIDTH}:-2:flags=lanczos,\
split[a][b];[a]palettegen=max_colors=${COLOURS}:stats_mode=diff[p];\
[b][p]paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle" \
        "media/${name}.gif"

    if [ -s "media/${name}.gif" ]; then
        printf '   ✓ media/%s.gif  (%s)\n' "$name" "$(du -h "media/${name}.gif" | cut -f1)"
    else
        echo "   ✗ the encode produced nothing." >&2
        exit 1
    fi
done

echo
echo "Done. Watch them, then uncomment the <img> tags in README.md."
echo "Especially sessions.gif: whatever the agents said is in your README now."
