#!/bin/sh
# Builds a believable workspace for the recordings.
#
# The tapes were spawning shells into an empty folder, which recorded an app
# with nothing in it. A screenshot of an empty app sells nothing.
#
# Everything lands under one throwaway directory, printed on stdout so a tape
# can `eval` it. Nothing touches a real vault or a real ~/.houston.

set -eu

ROOT="$(mktemp -d)"
VAULT="$ROOT/vault"
REPO="$ROOT/acme-api"
STATE="$ROOT/state"

mkdir -p "$STATE"

# ── A vault with enough in it to look lived in ────────────────────────────
mkdir -p \
    "$VAULT/Projects/acme-api/decisions" \
    "$VAULT/Projects/acme-api/research" \
    "$VAULT/Projects/billing" \
    "$VAULT/Knowledge/systems" \
    "$VAULT/Knowledge/howto" \
    "$VAULT/Daily" \
    "$VAULT/Workshop" \
    "$VAULT/Archive"

cat > "$VAULT/CLAUDE.md" <<'EOF'
# How agents use this vault

Before starting work on a project, read `Projects/<name>/index.md` and
everything in `decisions/`. Most big questions have been argued already, and
the losing argument is recorded alongside the winning one.

When you finish, append to `build-log.md` in the same commit as the work.
Newest first, dated absolutely. Only write an entry if it says something the
diff cannot: a dead end, a measurement that settled something, a library that
behaved unexpectedly.

Any load-bearing claim needs a number. "This is slow" needs a benchmark.
EOF

cat > "$VAULT/log.md" <<'EOF'
# Work log

## 2026-09-11
Token refresh rewritten. The old implementation refreshed on a timer and
raced itself under load; it now refreshes on 401 and serialises through a
single flight.

## 2026-09-09
Dropped the Dynamo migration. See `Projects/acme-api/decisions/0001`.
EOF

cat > "$VAULT/Projects/acme-api/index.md" <<'EOF'
# acme-api

The public API. Rust, axum, Postgres. Deployed from `main` on merge.

- Auth lives in `src/auth/`, and the token rules are in [[token-refresh]]
- Rate limiting is per-key, not per-IP. See [[rate-limiting]]
- Owner: platform team

## Where things are

| what | where |
|---|---|
| migrations | `migrations/`, sqlx, forward only |
| integration tests | `tests/`, needs a live Postgres |
| deploy | `.github/workflows/deploy.yml` |
EOF

cat > "$VAULT/Projects/acme-api/build-log.md" <<'EOF'
# Build log

## 2026-09-11
Refresh-on-401 landed. Measured: p99 auth latency 340ms -> 96ms under the
soak test, because the timer version was refreshing four times per minute
whether or not anything had expired.

## 2026-09-08
Tried moving sessions to Redis. Reverted. The serialisation cost was larger
than the Postgres round trip we were trying to avoid, which is the opposite
of what the RFC assumed.
EOF

cat > "$VAULT/Projects/acme-api/decisions/0001-postgres-over-dynamo.md" <<'EOF'
# 0001: Postgres over Dynamo

**Accepted, 2026-08-14.**

## The argument that won
We already run Postgres for everything else. One database is one thing to
back up, one thing to monitor, one set of failure modes people recognise.

## The argument that lost
Dynamo would scale further without thought. True, and irrelevant at our size:
the write rate would have to grow forty times before it mattered, and by then
this decision is cheap to revisit.
EOF

cat > "$VAULT/Projects/acme-api/decisions/0002-refresh-on-401.md" <<'EOF'
# 0002: Refresh tokens on 401, not on a timer

**Accepted, 2026-09-10.** Replaces the timer in [[token-refresh]].

A timer refreshes whether or not anything expired, and two requests landing
together both refresh. Refreshing on 401 through a single flight does neither.
EOF

cat > "$VAULT/Projects/acme-api/research/auth-latency.md" <<'EOF'
# Auth latency, 2026-09-11

Soak test, 200 rps for ten minutes.

| version | p50 | p99 |
|---|---|---|
| timer refresh | 88ms | 340ms |
| refresh on 401 | 71ms | 96ms |
EOF

cat > "$VAULT/Knowledge/token-refresh.md" <<'EOF'
# Token refresh

Access tokens last fifteen minutes. Refresh tokens last thirty days and
rotate on use, so a stolen refresh token is worth one request.

Refreshing happens on a 401 rather than on a timer. See
[[0002-refresh-on-401]].
EOF

cat > "$VAULT/Knowledge/rate-limiting.md" <<'EOF'
# Rate limiting

Per API key, not per IP, because customers sit behind shared egress and one
noisy tenant would otherwise take out a whole office.
EOF

cat > "$VAULT/Knowledge/systems/deploys.md" <<'EOF'
# Deploys

Merge to `main` deploys. There is no staging; there are feature flags.
EOF

cat > "$VAULT/Knowledge/howto/run-locally.md" <<'EOF'
# Running acme-api locally

`docker compose up -d` for Postgres, then `cargo run`. Integration tests
need the database up.
EOF

cat > "$VAULT/Daily/2026-09-11.md" <<'EOF'
# 2026-09-11

- Landed refresh-on-401, numbers in `research/auth-latency.md`
- Billing spike still parked, nobody has asked for it
EOF

cat > "$VAULT/Projects/billing/index.md" <<'EOF'
# billing

Parked. Picked up twice, dropped twice, for the same reason both times.
EOF

cat > "$VAULT/Workshop/agent-workflows.md" <<'EOF'
# Agent workflows, half-formed

Things that seem true after a few months of this:

- An agent that reads the decisions folder argues better than one that does not
- The log is worth more than the code comments, because it records what did
  not work
EOF

cat > "$VAULT/Archive/2025-old-api.md" <<'EOF'
# The old API

Retired 2025-11. Kept because search does not care about tidiness.
EOF

# ── A repository with something to diff ───────────────────────────────────
mkdir -p "$REPO/src/auth" "$REPO/migrations"
cd "$REPO"
git init -q -b main
git config user.email "demo@example.com"
git config user.name "Demo"

cat > src/auth/mod.rs <<'EOF'
pub mod refresh;

/// Fifteen minutes, matching the issuer.
pub const ACCESS_TOKEN_TTL_SECS: u64 = 900;
EOF

cat > src/auth/refresh.rs <<'EOF'
use std::time::Duration;

/// Refreshes the access token.
pub async fn refresh(token: &str) -> anyhow::Result<String> {
    let _ = Duration::from_secs(super::ACCESS_TOKEN_TTL_SECS);
    todo!("refresh {token}")
}
EOF

printf 'CREATE TABLE sessions (id uuid primary key);\n' > migrations/0001_sessions.sql
printf '# acme-api\n\nThe public API.\n' > README.md

git add -A
git commit -qm "Initial commit"

# Uncommitted work, so the diff column has something in it.
cat >> src/auth/refresh.rs <<'EOF'

/// Serialises concurrent refreshes so two requests cannot both refresh.
pub struct SingleFlight {
    in_flight: tokio::sync::Mutex<Option<String>>,
}
EOF
printf 'CREATE INDEX sessions_expires_idx ON sessions (expires_at);\n' > migrations/0002_index.sql

# New agent sessions default to the fixture repo. Without this the form
# defaults to $HOME, the recording shows somebody's actual home directory, and
# Claude Code greets them by name in a file destined for a public README.
cat > "$STATE/config.toml" <<CONF
theme = "Catppuccin Mocha"
vault = "$VAULT"
agent_directory = "$REPO"
CONF

printf 'ROOT=%s VAULT=%s REPO=%s STATE=%s\n' "$ROOT" "$VAULT" "$REPO" "$STATE"
