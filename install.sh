#!/bin/sh
# Installs Houston from the latest GitHub release.
#
#   curl -fsSL https://houston.sh/install | sh
#   curl -fsSL https://raw.githubusercontent.com/Sugar-Coffee/houston-ai/main/install.sh | sh
#
# Deliberately POSIX sh and nothing else: this is the one piece of Houston that
# runs before Houston exists, so it cannot assume bash, and it certainly cannot
# assume a Rust toolchain.
#
# Environment:
#   HOUSTON_INSTALL_DIR   where to put the binary (default: ~/.local/bin)
#   HOUSTON_VERSION       a specific tag, e.g. v0.2.0 (default: the latest)

set -eu

REPO="Sugar-Coffee/houston-ai"
INSTALL_DIR="${HOUSTON_INSTALL_DIR:-$HOME/.local/bin}"

say() { printf '  %s\n' "$*"; }
die() { printf '\n  %s\n\n' "$*" >&2; exit 1; }

need() {
    command -v "$1" >/dev/null 2>&1 || die "$1 is required and was not found."
}

need curl
need tar
need uname

# ── Which build? ──────────────────────────────────────────────────────────
target() {
    kernel="$(uname -s)"
    machine="$(uname -m)"

    case "$kernel" in
        Darwin)
            case "$machine" in
                arm64|aarch64) echo "aarch64-apple-darwin" ;;
                x86_64)        echo "x86_64-apple-darwin" ;;
                *) die "Houston has no macOS build for $machine." ;;
            esac
            ;;
        Linux)
            case "$machine" in
                x86_64) echo "x86_64-unknown-linux-gnu" ;;
                aarch64|arm64)
                    die "No Linux arm64 build yet. Build from source: cargo install --git https://github.com/$REPO"
                    ;;
                *) die "Houston has no Linux build for $machine." ;;
            esac
            ;;
        *)
            die "Houston runs on macOS and Linux. This looks like $kernel."
            ;;
    esac
}

TARGET="$(target)"

# ── Which version? ────────────────────────────────────────────────────────
if [ -n "${HOUSTON_VERSION:-}" ]; then
    TAG="$HOUSTON_VERSION"
else
    # The redirect on /releases/latest names the tag, which avoids needing a
    # JSON parser in a script that is meant to have no dependencies.
    #
    # It has to be matched rather than trimmed: a repository with no releases
    # does not redirect at all, and a blind `sed 's|.*/tag/||'` then hands back
    # the whole URL as though it were a version number.
    latest="$(curl -fsSLI -o /dev/null -w '%{url_effective}' \
        "https://github.com/$REPO/releases/latest" 2>/dev/null || true)"

    case "$latest" in
        */releases/tag/*) TAG="${latest##*/tag/}" ;;
        *) die "No releases published yet. Build from source:
    cargo install --git https://github.com/$REPO" ;;
    esac
fi

VERSION="${TAG#v}"
NAME="houston-${VERSION}-${TARGET}"
BASE="https://github.com/$REPO/releases/download/$TAG"

printf '\n  Houston %s for %s\n\n' "$TAG" "$TARGET"

# ── Fetch, verify, install ────────────────────────────────────────────────
TMP="$(mktemp -d)"
# shellcheck disable=SC2064
trap "rm -rf '$TMP'" EXIT INT TERM

say "Downloading…"
curl -fsSL "$BASE/${NAME}.tar.gz" -o "$TMP/houston.tar.gz" \
    || die "No build published for $TARGET at $TAG."

# Verifying is the whole reason the release publishes checksums. A download
# that half-succeeded is a file that extracts to nonsense, and a corrupted
# binary fails in ways nobody enjoys diagnosing.
if curl -fsSL "$BASE/${NAME}.tar.gz.sha256" -o "$TMP/houston.sha256" 2>/dev/null; then
    expected="$(cut -d' ' -f1 < "$TMP/houston.sha256")"
    if command -v shasum >/dev/null 2>&1; then
        actual="$(shasum -a 256 "$TMP/houston.tar.gz" | cut -d' ' -f1)"
    elif command -v sha256sum >/dev/null 2>&1; then
        actual="$(sha256sum "$TMP/houston.tar.gz" | cut -d' ' -f1)"
    else
        actual="$expected"
        say "No sha256 tool found; skipping verification."
    fi
    [ "$expected" = "$actual" ] || die "Checksum mismatch. Refusing to install."
    say "Checksum verified."
fi

tar -xzf "$TMP/houston.tar.gz" -C "$TMP"
[ -f "$TMP/$NAME/houston" ] || die "The archive did not contain a houston binary."

mkdir -p "$INSTALL_DIR"
# To a temporary name and then rename: replacing a binary that is currently
# running is fine on Unix, but only if the replacement is atomic.
cp "$TMP/$NAME/houston" "$INSTALL_DIR/houston.new"
chmod +x "$INSTALL_DIR/houston.new"
mv "$INSTALL_DIR/houston.new" "$INSTALL_DIR/houston"

say "Installed to $INSTALL_DIR/houston"

# ── Is it reachable? ──────────────────────────────────────────────────────
case ":$PATH:" in
    *":$INSTALL_DIR:"*)
        printf '\n  Run it with:  houston\n\n'
        ;;
    *)
        printf '\n  %s is not on your PATH. Add it:\n\n' "$INSTALL_DIR"
        printf '    echo '"'"'export PATH="%s:$PATH"'"'"' >> ~/.zshrc\n\n' "$INSTALL_DIR"
        printf '  Or run it directly:  %s/houston\n\n' "$INSTALL_DIR/houston"
        ;;
esac
