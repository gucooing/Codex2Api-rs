# Architecture

## Official protocol baseline

- Official repository: https://github.com/openai/codex
- Release: `0.160.0` / `rust-v0.160.0`
- Commit: `a956835d020762cb2b570053af06f643a11c0ecc`
- Commit time: `2026-10-01T17:13:37Z`
- Reference-only checkout: `reference/codex`; constants: `crates/codex2api-version`
- Maintained client: [gucooing/codex, ccodex](https://github.com/gucooing/codex/tree/ccodex)
- Fork integration commit: `232d7083cdd3a063f560e5f97a89c1b3bf8ee063`

The maintained client inherits the exact official release. Its customizations are
`BASE_OAUTH_URL`, the `ccodex` executable, `.ccodex` default home and its release
channel. Supplier protocol constants always come from official Codex. Official
Rust crates are never dependencies of this proxy. Follow [CODEX_UPDATES.md](CODEX_UPDATES.md)
for baseline changes; build, test and release checks run in cloud CI.

## Process and crates

One Rust process provides the public Responses API, administrator API and embedded
Next.js static export. Production does not run a Node.js server.

| Crate | Responsibility |
| --- | --- |
| codex2api-core | Provider identities, entitlement rules and model price presets |
| codex2api-storage | SQLite schema, credentials, configuration, resources and request ledger |
| codex2api-accounts | Persistent supplier identity and frozen per-account HTTP fingerprint |
| codex2api-auth | Supplier OAuth, token refresh/revoke and isolated account transports |
| codex2api-upstream | Official outbound endpoints, headers, body normalization, HTTP/SSE/WS |
| codex2api-service | Shared execution authorization and effective entitlement policy |
| codex2api-api | Consumer OAuth and ChatGPT client protocol adapters |
| codex2api-admin | Administrator session, CSRF, typed REST contracts and operations |
| codex2api-web | Embedded static assets and source freshness validation |
| codex2api-version | Exact official release and independent application release tag |
| codex2api | Startup, dependency composition, listener and graceful shutdown |

Incoming routes are explicit in `codex2api-api/src/providers/chatgpt/routes.rs` and
`codex2api-admin/src/rest/mod.rs`. Handlers adapt requests to services; they do not
derive routes from outbound endpoint enums. HTTP fingerprint and wire rules stay
in auth/upstream. SQLite code neither calls providers nor registers HTTP routes.

## Storage and supplier isolation

The default database is `data/codex2api.sqlite`. SQLx applies forward migrations.
Applied SQL remains immutable and uses LF, because migration checksums cover raw
bytes. Table rebuilds temporarily disable foreign keys only on a dedicated
connection that is closed after migration; normal application connections enforce
foreign keys. Never replace a live SQLite file to merge accounts or roll back code.

Each supplier has its own row, installation UUID, frozen OS/architecture/terminal,
HTTP fingerprint, tokens, cookie jar, HTTP clients and session state. There are no
per-supplier CODEX_HOME files. Upgrades update only version-derived UA values via
`align_user_agents`; they do not regenerate identity or read the proxy host as
the account's device.

Supplier ownership is `(provider_id, chatgpt_account_id, chatgpt_user_id)`.
A workspace can have several users, and one user can have several workspaces.
The workspace-scoped account-user membership claim is a separate value. Re-login
and refresh must preserve owner identity; missing user identity does not fall back
to email or workspace-only merging. Supplier IDs, credentials, routing revisions,
quota snapshots and historical attribution survive upgrades.

Consumer accounts, supplier accounts and administrator sessions are separate
entities. A consumer's provider is fixed at creation; execution routes may bind
only suppliers of that provider. Currently only ChatGPT has a protocol adapter.

## Upstream identity and transport

The supplier UA formula is
`codex_cli_rs/0.160.0 ({os_type} {os_version}; {arch}) {terminal_token}`.
Official headers include `originator`, `User-Agent`, bearer authorization,
`ChatGPT-Account-ID` and `x-codex-installation-id`; request-specific session,
thread, turn and request IDs retain their actual lifecycle.

OAuth uses `https://auth.openai.com`, client ID
`app_EMoamEEZ73f0CkXaXp7hrann`, and the pinned official scopes.
Responses uses `https://chatgpt.com/backend-api/codex/responses`.
Request normalization changes supplier identity metadata while preserving input,
tools, opaque Guardian messages and dynamic IDs. This is application-layer identity,
not forged TLS/JA3.

The CLI owns reconnect, backoff, queued-message recovery, generation retries and
transport fallback. The proxy does not replay generations after network failure,
SSE interruption, 429, 5xx or content filtering. It forwards structured failures
and retry headers; WebSocket failures end with the appropriate error/close path.

Supplier authentication can refresh a rejected token and retry once before a
generation/connection is established. This does not recover a broken established
stream. Reqwest's default safe HTTP protocol-NACK behavior remains in the transport
library; it is not an additional application retry loop.

Only an explicit upstream 401 rejects the corresponding supplier credential
revision. Network faults, timeouts, 403, 429, 5xx and malformed responses are request
failures. A delayed rejection of old credentials cannot disable newly refreshed
credentials. Administrator enable/disable policy is separate.

## Virtual-account data and management

### Ownership and subscription policy

Server-owned identity, catalog, billing, subscriptions, feature availability and
announcements are editable in administration. Client-owned sessions, projects,
pins, preferences, approvals, onboarding, installation state and operation history
are read-only in administration; normal client writes remain supported.
Mixed responses separate service flags from client preferences. Protocol fields
and identifiers are generated in code, not entered through arbitrary field editors.

Private data and settings belong to the virtual account in SQLite. Binding a
supplier authorizes execution, not inheritance of that supplier's private data,
usage, tasks, profile or quotas. Account/resource IDs in paths, queries and bodies
must pass ownership checks. Rebinding preserves virtual identity, configuration
and historical records.

Subscriptions are issued by administrators. Grants, renewals, plan changes and
expiration update the same persisted entitlements used by client responses and
execution checks. Expiration removes paid benefits but does not disable login.
Any free access follows an independent policy. Model access explicitly distinguishes
all enabled models, a selected list, and no models.

Management uses named fields and choices. Actual activity, requests, device
authorizations and resource operations are viewable as records; they cannot be
manufactured with configuration forms. Empty results are valid only after querying
an implemented data source and finding no records.

### OAuth and devices

Consumers log in with username/password through authorization code + PKCE S256.
Authorization uses state, callback validation, a cookie-bound CSRF flow and
single-use codes. Access and refresh tokens belong to the virtual device, never
to the supplier. Password changes, disabling/deleting an account and device
revocation invalidate the applicable credentials.

Virtual JWTs use the service's SQLite-backed RSA key and RS256/kid.
Access-token audience is an API audience array with granted scp values;
ID-token audience is the CLI client ID array and at_hash binds the actual access
token. Refresh preserves authentication time and session identity. The service
does not claim official signatures, MFA, payments or verified email that did not occur.

Device authorization stores short-lived pending codes and PKCE state in
`virtual_device_authorizations`. Browser approval uses the same password/CSRF
boundary and completes a normal virtual-device session. The administrator's
device list and revoke operation manage both browser and device-code logins.

### Supported models and price presets

The provider adapter's public, API-supported descriptors define the supported
business model registry. Hidden internal aliases are not automatically published.
Startup calls `sync_supported_models` before listening. It atomically registers
missing models and their verified presets; complete presets enable new models by
default. All-enabled plans then use their existing entitlement rule, while selected
plans retain their explicit lists.

Models with unknown prices remain visible but disabled and marked pending pricing.
When a later code version supplies a complete preset, only untouched automatically
pending records can be enabled. Existing custom prices, nonempty price sets, disabled
models, deleted tombstones, billing kinds and deliberately omitted tiers are
preserved. Startup does not reprice history or overwrite administrator decisions.

The administrator's model editor loads `GET /admin/api/models/presets`.
Entering an exact supported name fills the known price rules; administrators can
apply a preset explicitly or edit prices. Saved configuration remains the source
of truth. Preset requests carry a version; stale versions and mixed preset/custom
writes are rejected. Frontend fields are defined in code, not generated from
server-supplied field descriptions. Preset loading never calls the supplier.

Token rates use integer micro-USD per million tokens. Rules include Standard,
Fast/Flex, an input-token start and an optional inclusive upper bound. Migration
0049 adds the nullable upper bound without changing existing prices. Historical
snapshots without this field remain unbounded.

The shared presets include GPT-6 Astra, GPT-6.1 Sol, GPT-6 Sol/Luna, GPT-5.6
Sol/Terra/Luna and GPT-5.5. Prices have official source links and a verification
version. Public documentation is checked when maintaining presets, not fetched
during startup or administration.

GPT-5.5 Standard short-context rates are $5 input, $0.50 cached input and $30 output
per million tokens. It has no cache-creation surcharge; the ledger's effective
write rate equals ordinary input. Standard/Flex have their documented long-context
rules. Fast short-context rates are $12.50/$1.25/$75 and carry a 272,000 input-token
upper bound; a longer request is explicitly unpriced instead of inheriting that
short-context rate. See [pricing](https://developers.openai.com/api/docs/pricing)
and [prompt caching](https://developers.openai.com/api/docs/guides/prompt-caching).
Ultrafast and unsupported modalities are not silently mapped to other billing tiers.

Image billing uses actual successful image count and resolution bands based on
the longer edge: 0.5K/512, 1K/1024, 2K/2560, 4K/4096 and 8K/8192.
Existing exact-size snapshots retain their original prices. The current resolver
uses matching bands or the closest configured band; missing model prices or
unreported dimensions remain explicitly unknown. Do not derive service per-image
band prices from incompatible official token/quality units.

### Quotas and usage

Virtual quotas use model-priced USD costs, with at most two nested windows:
an outer 7-day or 30-day window beginning at subscription effectiveness, and an
optional 5-hour window beginning on use. Inner remaining funds cannot exceed outer
remaining funds; resetting the outer window clears the inner start. There is no
cumulative lifetime spending limit.

A request captures all model prices and context bounds at start. Settlement uses
actual ordinary input, cached input, cache writes and output, once; reasoning is
already included in output. Price edits, preset updates, rebinding and account
changes never alter that snapshot. Missing prices/usage remain unknown, not free.
Concurrent requests not yet settled can temporarily exceed a spending window.

Client usage JSON, HTTP headers, SSE and WS quota events read the same virtual
ledger. Client percentages are integers; local dollar summaries belong only in
administration. Supplier quota synchronization into virtual clients is prohibited.

Responses completion comes from terminal generation events, not HTTP 200 or WS 101.
HTTP JSON, SSE and WS share the outcome state machine. Later disconnects cannot
overwrite a completed generation. WS completion usage is persisted before releasing
completion to the client. Warmups and interrupts do not create another generation.

Request records contain model, actual model, token counts, cost snapshot, timing,
structured error cause and official upstream request ID. They do not retain prompts,
response text, image bytes or credentials. Error text is bounded and redacted.
First-byte timing measures the first meaningful generation event, including
response.created, rather than necessarily the first visible text token.

Administrator tables and charts query actual records and preserve unknown fields.
The official reported model in response headers takes precedence over payload
model names; requested and actual model remain separate. Upgrade headers do not
become generation-scoped IDs for later WebSocket generations.

Reset cards and direct administrator quota resets preserve subscription expiration
and historical costs, atomically move the current quota generation, and deduplicate
requests. In-flight requests retain their original generation. See
[RESET_CREDITS.md](RESET_CREDITS.md) for current operations and response contracts.

### Supplier quota snapshots

Supplier lists read SQLite snapshots. Missing/expired quota data uses the existing
ten-minute cache and per-account lock; failures retain the previous snapshot.
UI filters, view changes and countdown ticks never call the provider.

Render only windows actually returned, using their reported duration and reset
time, including 30-day windows. Do not invent absent primary/secondary windows.
Supplier cycle summaries aggregate local execution records using the actual
snapshot boundaries and request-start prices. They remain distinct from virtual
consumer quotas.

## Administration and frontend

The single administrator uses a password hash and session cookie. Default
credentials are admin/admin on first initialization. Mutations require the
administrator CSRF token; consumer bearer tokens never authorize administration.

Next.js exports static assets embedded by codex2api-web. Its build script verifies
the source manifest and rejects missing/stale assets. HTML is not cached; hashed
assets can be. Unknown API routes never fall back to the page shell.

Use official shadcn components, fixed frontend structure, browser-local neutral
themes and small floating error toasts. Refresh failures retain data and edits.
Writes require real loaded data; placeholder defaults are never saved. See
[FRONTEND_UI.md](FRONTEND_UI.md).

## Client routes and capability boundaries

Public inference is POST /v1/responses; the /api/oauth/chatgpt service root mounts
OAuth and the compatible backend-api/v1 routes. HTTP, SSE and WebSocket use the
same execution authorization and ownership rules. Reverse proxies must preserve
paths, support WebSocket and provide TLS when required. PUBLIC_BASE_URL declares
an externally reachable origin; it does not start a TLS server.
See [CCODEX_ENDPOINTS.md](CCODEX_ENDPOINTS.md) for the current route groups.

Locally recorded tasks, conversations, events, profiles and statistics are scoped
to the consumer. Cloud operations require actual successful upstream execution
before recording a resource. Rebinding must not use a different supplier to read
old private resources.

File create/upload/finalize/download, external plugin/MCP authorization/execution,
cloud automation scheduling, purchases and several external product operations
remain incomplete. Displaying local records or routing a URL does not implement
those capabilities; missing operations remain explicit.

Managed network policy does not provide complete instantaneous revocation of
already-running HTTP streams. WS checks account/routing revisions at message
boundaries. CLI mock/contract tests and cloud builds do not prove successful live
provider operations or Desktop execution. The native C# launcher has its own
compatibility boundaries and is maintained separately.
