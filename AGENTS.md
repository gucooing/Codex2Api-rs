# Codex2API implementation notes for agents

This repo is a Rust proxy that converts ChatGPT/Codex subscriptions into a Codex-compatible Responses API.

## Pinned official Codex version

All protocol/header/OAuth/upstream behavior MUST match official Codex at:

- repo: https://github.com/openai/codex
- path: `reference/codex`
- commit: `00c972ed5d6ff6499317fd41b7f23605b8e6850d` (2026-09-24T18:33:35-07:00)
- constants: `crates/codex2api-version`

Do not depend on official `codex-rs` crates. Official source is reference only.

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
- package version in UA: `0.157.0`
- OAuth client_id: `app_EMoamEEZ73f0CkXaXp7hrann`
- issuer: `https://auth.openai.com`
- token: `https://auth.openai.com/oauth/token`
- ChatGPT Codex base: `https://chatgpt.com/backend-api/codex`
- responses path: `/responses`
- OAuth scopes: `openid profile email offline_access api.connectors.read api.connectors.invoke`
- headers: `originator`, `User-Agent`, `Authorization: Bearer`, `ChatGPT-Account-ID`, `x-codex-installation-id`
- installation_id: UUID persisted on the account SQLite row

## Coding rules

### Frontend

- Keep Next.js static export embedded in the single Rust process; production must not require a Node.js server.
- Use official shadcn/ui components directly. Do not create custom visual wrappers, controls, or handwritten component CSS. Keep only required business data/state/event logic outside the official component source.
- Keep all pages compact. Do not add duplicate page-title/description banners in the content area; put refresh, view switches and create actions inside the top filter bar.
- Supplier quota summaries stay compact: render the windows actually returned by the official response, labelled by their reported duration (including 30-day windows), without placeholders for absent windows. Each window has reset countdown on the first line and shadcn Progress plus percentage on the second; no extra label before the percentage. Read persisted official snapshots and reuse the existing cache; UI countdown/filter/view changes must not call the provider.
- Frontend code owns page structure and field definitions. Never gate entire pages, tables, tabs or forms on API data/loading/error, or use backend field descriptors to construct the UI. Render structure first and bind values/rows as they arrive. Keep same-resource data on refresh failure and disable writes until actual data is ready. Never save placeholder defaults.
- Use the official shadcn/ui neutral palette; do not add custom accent-color choices or overrides. Light, dark and system theme modes are browser-local preferences; persist the mode locally, never in a business account or server policy.
- Show loading, request, login, save and validation failures as small floating toast messages. Error feedback must not occupy page/form layout, move keyboard focus or use a blocking browser validation popup; persistent business statuses remain normal record content.
- See [Frontend UI](docs/FRONTEND_UI.md) for the component and interaction conventions.

### Virtual-account client data (user-confirmed 2026-09-20)

Ownership clarification (user-confirmed 2026-09-21): admin editing applies to
server-owned identity, catalog, billing and service policy. Client-owned sessions,
sidebar projects/pins, preferences, approvals, onboarding, installation state and
operation history are read-only in admin; preserve their normal client writes.
Do not manufacture client state through configuration forms. Mixed responses must
separate editable service flags from client preferences. Generate protocol fields
in code and offer named business controls, never arbitrary field-name editors.
This clarification supersedes the older blanket requirement to edit every client value.

Subscription operations clarification (user-confirmed 2026-09-21): operate each
virtual account as a subscription account, following the ChatGPT subscription
account model; subscriptions are manually issued by the administrator instead of
purchased through an official checkout. Subscription grants, renewals, plan changes
and expiration are service-owned business state, not cosmetic display fields.
Admin operations, persisted entitlements, client responses and execution checks
must agree. Expiration must end the expired subscription's benefits; any remaining
free-tier access follows the service policy and is distinct from disabling login.
Keep client-owned state under the ownership rule above. Every requested subscription
or endpoint repair must include its corresponding administration operation or record
view; do not fabricate purchases, payments or supplier-account entitlements.

Before changing virtual-account endpoints, read
[Virtual-account data and management](docs/ARCHITECTURE.md#virtual-account-data-and-management).

- Client identity, private data, preferences and feature configuration belong to the
  virtual account. Configure and persist them locally in SQLite and expose them in
  the admin web UI. Do not inherit a bound upstream account's private data.
- Private data is locally configurable virtual data; do not unconditionally blank
  it or disable its feature merely because the account is virtual.
- Usage, tasks, activity and logs must reflect the virtual account's own actual
  records. Replacing upstream identity fields does not establish data ownership.
- Supplier quota synchronization has been removed. Never restore `sync_quota`,
  fixed usage percentages, or passthrough supplier quotas for virtual clients.
  Read virtual spending limits and actual billed usage from SQLite, including HTTP headers,
  SSE events, WebSocket events and quota pages. Supplier binding is for execution.
- Virtual quota has at most two nested windows, using model-priced USD costs:
  the outer window is 7 days or 30 days, and the optional inner window is 5
  hours. The inner window is constrained by the outer remaining amount. The
  outer window starts when the subscription becomes effective; the inner
  window starts on use and is cleared when the outer window resets. The user
  explicitly removed the cumulative total spending limit. Never restore it in
  settings, validation, enforcement or client responses.
  Prices are managed under Settings / Model
  billing. Snapshot prices at request start, settle reported ordinary input,
  cached input, cache writes and output once, and never charge reasoning twice.
  Price edits must not reprice history. Unknown prices/usage remain explicit;
  never treat them as free or convert historical Token limits directly to USD.
- Desktop quota responses require integer `used_percent` values; floating JSON
  numbers fail the installed app-server reader even when renderer selectors pass.
  Validate quota startup reads with both actual renderer selectors and the
  installed Desktop app-server. Preserve virtual account/user identity and reset
  timestamps, and propagate the same windows through headers and SSE/WS events.
  Client usage responses expose the actual Desktop protocol fields only; local
  billing summaries and dollar-denominated administration fields stay in admin.
- WebSocket application failures must retain their structured error/status and
  send a close frame. Check budget before upgrading and before each generation;
  never turn quota rejection into a bare TCP reset. Persist completed WebSocket
  usage before releasing completion to the client.
- Client-required configurable data must be viewable/editable in the admin web UI;
  actual activity/statistics must be inspectable there. Client and admin must use
  the same persisted source of truth. No hidden hard-coded client configuration.
- Virtual-account administration uses a horizontal tab row on the account detail
  page. Render configuration as labelled inputs, switches, selects and editable
  lists; render usage as charts and records as tables. Reuse the existing admin
  components/styles. Do not substitute raw JSON editors or JSON dumps for the UI.
- For every client request being added or changed, inspect the installed Codex
  Desktop's request construction, response readers and downstream branches; verify
  against that actual Desktop logic. Codex CLI source is not a substitute for
  Desktop evidence. Cover required/optional fields, types, pagination, identity,
  errors and request sequencing before marking the endpoint fixed.
- Update the corresponding admin UI alongside endpoint work. Define business
  fields and choices up front. Generate protocol/account/device identifiers in
  code; never require users to invent field names, fill protocol metadata or edit
  JSON to make the client work.
- This project emulates the server for virtual accounts. Fix server contracts;
  do not patch Desktop business logic or bypass its readiness/permission checks.
  Our launcher must clean up its own temporary debugging environment, including
  Worker inheritance of its `--inspect-brk` flag. Report server, launcher and
  actual Desktop execution evidence separately; parser tests alone do not prove
  that a task was created or that an inference request was sent.
- Desktop launcher policy (user-confirmed 2026-09-21): use the original client's
  default credentials, data directory and native runtime. Keep only address
  routing hooks; do not create isolated profiles, copy runtime executables, wrap
  app-server with a shim, or force credential storage. The only login option
  override is disabling the official hosted success-page redirect so login stays
  on the custom service/local callback. Preserve other login options.
  Login/authorization, token refresh and revoke must stay on the configured proxy;
  preserve PKCE, state and callback parameters. Fix response schemas in the server.
- The production launcher is the compiled C# GUI in `tools/desktop-proxy`.
  Discover client paths/versions from installed package manifests; make server
  addresses and manual paths configurable. Do not hard-code device paths, server
  addresses or a client build. Embed the address hook in the distributable EXE;
  fail with a visible compatibility error when an unknown client changes its hooks.
- An endpoint is not complete just because it returns 200 or a schema-valid empty
  response. An empty result is valid only after querying the implemented account
  data source and finding no records. Missing capabilities remain explicitly
  unfinished until persistence, management and client behavior are connected.
- Existing placeholders and supplier-data forwarding are known gaps, not patterns
  to copy. Follow the documented repair inventory; do not invent usage, task/log
  history, official grants, billing transactions or external-service success.
- These requirements do not authorize unrelated feature expansion or a Codex
  baseline update. Implement each requested repair through its data and UI path.

- Match existing crate boundaries.
- Keep account isolation. Never share auth, installation_id, HTTP client, or session state.
- Persist identity; do not regenerate installation_id on re-login of the same ChatGPT account.
- Dynamic IDs (session/thread/turn/request) stay request-scoped.
- Write compiling Rust (edition 2024). Prefer `anyhow`/`thiserror`, `axum` 0.8, `sqlx` sqlite, `reqwest` rustls.
- After your crate is implemented, `cargo check -p <crate>` should pass.
