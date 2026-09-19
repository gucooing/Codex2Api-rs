# Architecture

Implementation is aligned with official Codex:

- repo: https://github.com/openai/codex
- commit: `a8964cb1bad67bc26a826fb07d1bef99c6a3f008`
- date: 2026-09-15T05:44:41Z
- snapshot: `reference/codex`
- constants: `crates/codex2api-version`

Official source is reference only. Do not depend on `codex-rs` crates.

An administrator may opt an individual account into experimental HTTP turn-state
reuse. This defaults off and is an explicit exception to the official per-turn
routing contract, not a new aligned baseline. Settings, cache and probe leases
are in SQLite; tokens never cross accounts. See [implementation notes](TURN_STATE_REUSE_PLAN.md).

## Process

One Rust process:

- public Codex-compatible Responses API
- admin web UI (one admin user)
- SQLite persistence
- per-account isolated upstream Codex environments

## Crates

| crate | responsibility |
| --- | --- |
| `codex2api-version` | pinned Codex constants |
| `codex2api-storage` | SQLite schema, migrations, admin user, account rows, tokens, sessions |
| `codex2api-accounts` | per-account SQLite identity, installation_id, frozen HTTP fingerprint |
| `codex2api-auth` | ChatGPT OAuth PKCE login, token refresh/revoke, SQLite token persistence |
| `codex2api-upstream` | talk to official Codex servers with official headers/body/stream behavior |
| `codex2api-api` | all incoming Codex-client routes and HTTP/WebSocket handlers |
| `codex2api-admin` | administrator routes, session authentication and HTML views |
| `codex2api` | `lib.rs` composes the app; `main.rs` owns startup, listening and shutdown |

## Module organization

The HTTP entry points are `codex2api-api/src/lib.rs` and `codex2api-admin/src/lib.rs`.
They explicitly register each method, path and handler. Public route registration
does not iterate upstream enums or derive incoming paths from official outgoing URLs.

| Location | Responsibility |
| --- | --- |
| `codex2api-api/src/lib.rs` | public API route inventory, compatibility mounts and body limit |
| `codex2api-api/src/handlers/` | Codex HTTP, backend, realtime, WebSocket and process metadata handlers |
| `codex2api-api/src/state.rs`, `auth.rs`, `response.rs`, `error.rs` | dependencies, API-key authentication, response forwarding and API errors |
| `codex2api-admin/src/lib.rs` | public/protected admin routes and session middleware attachment |
| `codex2api-admin/src/handlers/` | login, accounts/API keys, OAuth and official-account operations |
| `codex2api-admin/src/services.rs` | official account data shared by account details and usage pages |
| `codex2api-admin/src/state.rs`, `session.rs` | shared dependencies, OAuth progress and session authentication |
| `codex2api-admin/src/models.rs`, `views/`, `static/` | page data, escaped HTML and static assets |
| `codex2api-upstream/src/lib.rs` | outbound client/pool/protocol exports; no incoming routes |
| `codex2api-auth/src/lib.rs` | OAuth orchestration, persistence and account transport exports |
| `codex2api-accounts/src/lib.rs` | persistent account identity and account-store exports |
| `codex2api-storage/src/lib.rs` | SQLite records, repository and credential-helper exports |
| `codex2api/src/lib.rs` | construct one shared account/auth/upstream context and merge both HTTP surfaces |

Handlers adapt incoming requests to services. Views render data without making
upstream requests. Official URL and wire-protocol rules remain in upstream/auth;
SQLite persistence remains in storage/accounts. Existing public Rust exports stay
available so callers do not depend on handler or template file locations.

## Storage

SQLite file default: `./data/codex2api.sqlite`

Startup applies SQLx migrations automatically. Applied migration files remain immutable:
feature removal is a new forward migration, not deletion of migration history. Migration
`0005_remove_usage_errors.sql` removes the reverted error-detail column, including databases
where the column was already manually removed, while retaining all usage rows and indexes.
If the usage table itself was removed while migration history remains, that migration
recreates the empty table and its indexes without changing account or credential tables.

Account identity (`installation_id`, originator, User-Agent, OS/arch), official CLI HTTP fingerprint, and tokens are stored on the account SQLite row / `account_tokens`. There is no per-account `$CODEX_HOME` directory. Isolation is a database row, not a filesystem tree.

HTTP fingerprint is application-layer only (same headers Codex CLI sends): `originator`, `User-Agent`, `x-codex-installation-id`, plus this account's cookie jar. It is captured once at account creation and reused; it is not a forged TLS/JA3 device fingerprint.

Admin:

- single user
- username/password
- default `admin` / `admin` created on first boot if no admin exists
- password stored as argon2id hash

## Isolation

Each upstream ChatGPT/Codex account has:

- own SQLite row
- own `installation_id` (column on that row)
- own frozen HTTP fingerprint (`http_fingerprint_json`)
- own tokens (`account_tokens`)
- own HTTP client / cookie jar
- own session/thread/turn state

Do not share those across accounts.

## Upstream identity (must match official Codex CLI)

From `reference/codex` at the pinned commit:

- originator: `codex_cli_rs` (constant)
- User-Agent formula (official `get_codex_user_agent`): `{originator}/{CARGO_PKG_VERSION} ({os_type} {os_version}; {arch}) {terminal_token}`
- UA version token is the packaged release `0.154.0` (constant)
- OS / arch / version / terminal are rolled once per account from official `os_info` + terminal-detection value sets, then frozen on that account row. Same account always sends the same UA. Do not read the proxy host.
- ChatGPT Codex base: `https://chatgpt.com/backend-api/codex`
- Responses path: `/responses`
- OAuth issuer: `https://auth.openai.com`
- client_id: `app_EMoamEEZ73f0CkXaXp7hrann`
- token URL: `https://auth.openai.com/oauth/token`
- headers include `originator`, `User-Agent`, `Authorization: Bearer`, `ChatGPT-Account-ID`, `x-codex-installation-id`
- dynamic ids (`session-id`, `thread-id`, `x-client-request-id`, turn metadata) are per-request, generated inside the account context

## Public API

Codex clients should be able to point `base_url` at this proxy with `wire_api = "responses"`.

`codex2api-api/src/lib.rs` is the authoritative route inventory:

- `/v1`: Responses/Guardian HTTP and WebSocket, models, search, images, memory summaries and realtime.
- `/backend-api/codex`: compatibility mount of the same Codex routes.
- `/backend-api/wham`: account checks, profile/config/settings, messages, usage details, reset credits and cloud tasks.
- `/wham`, `/api/codex`, `/v1/api/codex`, `/v1/wham`: compatibility mounts of the WHAM routes.
- `GET /v1/usage`: short alias for WHAM usage.
- `GET /healthz` and `GET /version`: unauthenticated process metadata.

API key for public clients is a proxy-issued token bound to one upstream account.

## Admin

- `GET /admin/login` form
- `POST /admin/login` username/password
- cookie session
- pages to start Codex OAuth, list accounts, enable/disable, issue proxy API keys, inspect status
- default credentials: `admin` / `admin`

## Client usage ledger

- `codex2api-api/src/usage.rs` observes incoming billable Codex HTTP requests and each
  Responses WebSocket `response.create`. WebSocket `generate=false` warmups are excluded.
- `codex2api-storage/migrations/0003_usage_records.sql` stores account/key label snapshots,
  endpoint, model, requested reasoning effort, official token counters, image size,
  first-body-byte latency, total forwarding duration, request timestamp and completion state.
- SSE/JSON bytes are forwarded unchanged. Counters are read from upstream completion events;
  missing values remain NULL. Input includes cached input; output includes reasoning tokens.
  Search usage displays a dash; image requests display their size.
- First-byte timing begins when the handler receives an HTTP request (or a WebSocket generation)
  and ends at the first upstream body bytes/event. Total time ends at HTTP stream completion or
  a WebSocket terminal event. Internal auth retries do not create extra client request records.
- The record is inserted before forwarding; completion is persisted asynchronously. Client
  disconnects finalize interrupted records, and process startup marks previously unfinished rows
  interrupted. No historical usage is inferred from the official aggregate profile.
- `/admin/usage` provides session-protected filtering by API Key, account name, model and UTC
  time range, with browser-local time display and 50-row pagination. Existing official quota
  and daily-profile charts remain separate from this locally collected request ledger.
- Only client traffic is recorded; admin reads, key/token secrets, prompts, response text and
  image content are not stored in the ledger.
