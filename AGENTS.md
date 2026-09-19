# Codex2API implementation notes for agents

This repo is a Rust proxy that converts ChatGPT/Codex subscriptions into a Codex-compatible Responses API.

## Pinned official Codex version

All protocol/header/OAuth/upstream behavior MUST match official Codex at:

- repo: https://github.com/openai/codex
- path: `reference/codex`
- commit: `a8964cb1bad67bc26a826fb07d1bef99c6a3f008` (2026-09-15T05:44:41Z)
- constants: `crates/codex2api-version`

Do not depend on official `codex-rs` crates. Official source is reference only.

Explicit optional exception: per-account experimental turn-state reuse may replay
same-account/same-model state across turns on HTTP Responses when enabled by the
administrator. It defaults off, collects state only from normal HTTP traffic,
and preserves eligible caller-provided state while refreshing the cache. Cached
state only replaces missing or ineligible caller state. It does not change
WebSocket behavior or send auxiliary probes. See `docs/TURN_STATE_REUSE_PLAN.md`. Do not describe
this experiment as official protocol alignment or proven model-quality improvement.

## Updating the official Codex baseline

When the user requests a Codex update, upgrade, or sync, read and follow
[docs/CODEX_UPDATES.md](docs/CODEX_UPDATES.md) before making changes.
Compare the current `CODEX_REF_COMMIT` with the exact commit of the requested
official release, inspect actual client behavior, and write an evidence-backed
proposal in `docs/CODEX_UPDATE_PLAN.md`. An update request authorizes investigation
and the proposal; wait for the user's decision on that concrete proposal before
changing implementation, constants, dependencies, persisted data, or the reference
snapshot. Honor an existing explicit approval of that proposal without asking again.
Never substitute a UA version bump or release-note summary for protocol analysis.
Only mark a new baseline as aligned after the approved compatibility work and its
validation are complete.

## Product constraints

- One process.
- SQLite storage (`data/codex2api.sqlite`).
- Isolated persistent per-account identity and HTTP fingerprint in SQLite (no `$CODEX_HOME` files).
- HTTP fingerprint: official header names/formula; originator + UA version are constants; OS/terminal are random per account then frozen. Do not use the proxy host. Do not forge TLS/JA3.
- Admin web: one user, username/password, default `admin` / `admin`.
- Public API is Codex-compatible `POST /v1/responses`.
- Upstream requests must look like a logged-in official Codex CLI for the pinned commit.

## Official constants (do not invent)

- originator: `codex_cli_rs`
- package version in UA: `0.154.0`
- OAuth client_id: `app_EMoamEEZ73f0CkXaXp7hrann`
- issuer: `https://auth.openai.com`
- token: `https://auth.openai.com/oauth/token`
- ChatGPT Codex base: `https://chatgpt.com/backend-api/codex`
- responses path: `/responses`
- OAuth scopes: `openid profile email offline_access api.connectors.read api.connectors.invoke`
- headers: `originator`, `User-Agent`, `Authorization: Bearer`, `ChatGPT-Account-ID`, `x-codex-installation-id`
- installation_id: UUID persisted on the account SQLite row

## Coding rules

- Match existing crate boundaries.
- Keep account isolation. Never share auth, installation_id, HTTP client, or session state.
- Persist identity; do not regenerate installation_id on re-login of the same ChatGPT account.
- Dynamic IDs (session/thread/turn/request) stay request-scoped.
- Write compiling Rust (edition 2024). Prefer `anyhow`/`thiserror`, `axum` 0.8, `sqlx` sqlite, `reqwest` rustls.
- After your crate is implemented, `cargo check -p <crate>` should pass.
