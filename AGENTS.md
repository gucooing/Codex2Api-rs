# Codex2API implementation notes for agents

This repo is a Rust proxy that converts ChatGPT/Codex subscriptions into a Codex-compatible Responses API.

## Pinned official Codex version

All protocol/header/OAuth/upstream behavior MUST match official Codex at:

- repo: https://github.com/openai/codex
- path: `reference/codex`
- commit: `979011409de0a60b52f179721948e65531d26144` (2026-10-06T22:34:37Z)
- constants: `crates/codex2api-version`

Do not depend on official `codex-rs` crates. Official source is reference only.

## Updating the official Codex baseline

The maintained client is https://github.com/gucooing/codex, branch `ccodex`
(a sibling checkout such as `../codex`). Update that fork from an exact official
stable release first, then compare and adapt this proxy. Record both the official
release commit and the fork integration commit; fork-only service routing,
`BASE_OAUTH_URL`, the `ccodex` executable and npm packaging must never replace
official supplier-side protocol constants. The client otherwise retains official
Codex behavior and isolates its default home from official Codex, not per service.

Current official release integration in the fork: `9995ddd7cd50b9397305a76b1fdac6d26f435588`.

Validate proxy changes locally with the relevant compilation checks and regression
tests before reporting completion. Do not compile ccodex locally; use source review,
formatting and package checks that do not compile, and record its Rust validation as
unverified. Do not push changes or create branches solely
to obtain cloud CI validation. Record skipped or failed checks honestly instead
of claiming validation passed.

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
- User web/API, administrator web/API, and AI API have separate routers and
  configurable listening ports in the same process. Build the two Next.js
  frontends independently; never mount administrator routes on other listeners.
- Ordinary and virtual users share `accounts`/`users` identity storage, with immutable
  `users.kind` (`regular`/`virtual`). Their business domains remain independent.
  Virtual users can authenticate only through OAuth, never the user website; manage
  them only in administrator Virtual accounts. Never include virtual users in regular
  user management, catalogs, wallets, orders, user records or user statistics.
  Use the storage `require_account_scope` guard, scoped database views and the
  account-scope middleware on whole routers; keep SQL triggers enforcing session,
  wallet, order and ownership restrictions even if a handler omits a check.
  Only administrators create users. Each regular user has one subscription identity per
  platform, initially Free. Expired paid benefits resolve to the platform Free
  plan without changing identity or history. Free plans cannot be deleted or
  changed to a paid tier. Do not restore paid-plan-specific free-access settings.
- Wallet balances and sale prices use USD and integer cents. Administrator-only
  wallet adjustments must atomically change the balance and append an immutable
  system-operation ledger entry with operator, optional reason and before/after amounts;
  require a revision and idempotency key. No user funding route exists. Plan allow_purchase controls user catalog publication/purchase only;
  administrator assignment must not depend on it. Preview is read-only and must
  precede explicit order confirmation, separate payment and completion. Coupons
  reserve only on confirmation and keep immutable discounts on confirmed orders;
  payment atomically validates revisions, debits the wallet, updates entitlements
  and saves an idempotent ledger entry. Upgrades compare daily prices and credit
  remaining original price snapshots, including administrator grants; keep the
  expiry/spending anchor. Downgrades wait for expiry. Never invent missing prices,
  historical orders or successful payments. User usage queries always bind ownership
  and expose only local, user-visible ledger fields; dashboard charts must preserve
  unknown measurements and never read supplier usage.
- User DTOs use explicit field allowlists. Never expose supplier identities,
  credentials, fingerprints, quota snapshots or routing through user APIs or
  client tokens. Browser sessions and AI credentials have separate audiences.
  OAuth identity verification and confirmation are separate steps; temporary
  login applies only to the current authorization link.
- Login identity lives in accounts (immutable account_type admin/user); admin_users
  and users are separate role-checked profiles. Platform accounts hold entitlements,
  never separate usernames/passwords. Migrate old identities with SQL only; do not
  add runtime legacy authentication, dual writes or old-schema compatibility. The user HTTP module receives only
  UserStore, never raw storage or supplier/admin query capabilities.
- Administrator/user web session tokens are purpose-bound JWTs with independent admin
  and user RSA keys. Allow only the server's fixed RS256 algorithm; reject none,
  empty/other algorithms and token-selected key sources. Check account_type,
  token_use, audience, issuer, lifetime and persisted revocation/ownership.
  Do not accept old opaque web sessions or fallback to a cookie
  when an explicit Bearer credential fails. Codex OAuth is a separate client
  protocol: preserve its official claims, signing keys, refresh/revoke behavior and
  auxiliary credential formats; never apply web-session JWT changes to it.
- Public API is Codex-compatible `POST /v1/responses`.
- Upstream requests must look like a logged-in official Codex CLI for the pinned commit.
- Ordinary supplier request throttling (`rate_limit_exceeded`, `slow_down`, or an
  unclassified HTTP 429) belongs to client backoff. Forward its structured error
  and retry information; never persist a supplier throttled state or switch
  suppliers for it. Pool failover is for unrecoverable authorization failure,
  explicit quota exhaustion, or ChatGPT HTTP 402 billing rejection. A ChatGPT
  402 persists as payment/subscription unavailable without deleting credentials,
  refreshing tokens to recover it, or expiring it as a quota cooldown. Only a
  successful explicit reauthorization or administrator reset clears that state.
- Supplier enablement is administrator intent, independent of observed availability.
  Do not block enable/disable operations on authorization, billing or quota state.
  Successful reauthorization automatically clears old failure state and preserves
  manual disablement and frozen identity. Do not require a manual authorization
  recovery button. Late failures or token refreshes must not overwrite a new login.

## Official constants (do not invent)

- originator: `codex_cli_rs`
- package version in UA: `0.161.0`
- OAuth client_id: `app_EMoamEEZ73f0CkXaXp7hrann`
- issuer: `https://auth.openai.com`
- token: `https://auth.openai.com/oauth/token`
- ChatGPT Codex base: `https://chatgpt.com/backend-api/codex`
- responses path: `/responses`
- OAuth scopes: `openid profile email offline_access api.connectors.read api.connectors.invoke`
- headers: `originator`, `User-Agent`, `Authorization: Bearer`, `ChatGPT-Account-ID`, `x-codex-installation-id`
- installation_id: UUID persisted on the account SQLite row

## Coding rules

### Provider boundaries

- Keep channel code in each relevant crate's `src/providers/chatgpt/` or
  `src/providers/grok/`. Frontend channel forms, labels and protocol views belong
  in `frontend/src/components/providers/chatgpt/` or `providers/grok/`.
- A channel may not import another channel or add branches to the other channel's
  authentication, transport or handlers. Root modules may dispatch by provider;
  shared SQLite, supplier pools, billing, sessions and UI layout stay protocol-neutral.
- Common supplier error handling is common within a provider: ChatGPT quota
  probes and administrator refreshes reuse ChatGPT execution's credential refresh,
  retry and error classification. Never apply ChatGPT error codes or recovery
  rules to Grok or another provider; only classified availability reaches shared storage.
- Grok supplier credentials, PKCE/device/RT flows, frozen identity, clients and
  version constants are independent of ChatGPT. Never interpret Grok tokens as
  Codex auth.json or send Codex headers to Grok.
- The administrator's enabled model catalog determines availability, subject to
  the consumer's plan. Official catalogs and price presets are references, never
  model allowlists. Custom models must remain addable and usable. Missing prices
  stay unknown; known prices snapshot at request start.
- RT creation accepts one token per line. New accounts independently generate
  device fingerprints and installation IDs; reauthorization preserves the existing
  account identity. Never print, persist in browser preferences, or echo RT values
  in batch results.
- Filter dropdowns follow the administrator usage-record filter form: official
  shadcn Select/Combobox, compact labelled fields, full-width triggers and popper
  dropdowns. Do not introduce alternate filter controls or visual wrappers.

### Documentation lifecycle (user-confirmed 2026-10-03)

- Keep documentation focused on the current architecture, behavior, contracts and maintenance rules.
- Update plans are temporary working documents used for approval and implementation. After the task is complete, remove the completed plan and historical version plans, investigation notes, audit logs and execution diaries; do not archive them as new Markdown files.
- Merge enduring conclusions and known limitations into the relevant current documentation, then remove links to deleted records. Git history and cloud CI retain the change and validation history.
- Do not add dated progress logs or prose such as "the previous implementation record has been archived" to README or architectural documentation.

### Frontend

- Keep Next.js static export embedded in the single Rust process; production must not require a Node.js server.
- Use official shadcn/ui components directly. Do not create custom visual wrappers, controls, or handwritten component CSS. Keep only required business data/state/event logic outside the official component source.
- Keep all pages compact. Do not add duplicate page-title/description banners in the content area; put refresh, view switches and create actions inside the top filter bar.
- Keep implementation explanations and long operating instructions in documentation,
  not on web pages. Use concise field labels, actual values and action feedback;
  do not render developer notes about protocol, persistence, isolation or bookkeeping.
- Supplier quota summaries stay compact: render the windows actually returned by the official response, labelled by their reported duration (including 30-day windows), without placeholders for absent windows. Each window has reset countdown on the first line and shadcn Progress plus percentage on the second; no extra label before the percentage. Read persisted official snapshots and reuse the existing cache; UI countdown/filter/view changes must not call the provider.
- Supplier lists read only snapshots included in the list response. Missing or stale
  snapshots must not trigger quota requests. Do not add a list refresh button or
  batch quota refresh; official reads belong to background probes and explicit
  account-detail refreshes.
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
account model. Standalone accounts are administered directly; user subscriptions
are administrator grants or local-wallet purchases, never official checkout payments.
Subscription grants, renewals, plan changes
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
  Launcher loading and routing must be implemented in C#, with no JavaScript/CJS
  payloads, script evaluation, debugger protocol, inspector ports or breakpoints.
  HTTP scheme compatibility in workspace discovery and subsequent native routing
  is explicitly authorized by the user on 2026-09-26; keep identity, credentials,
  account ownership and permission checks. Do not substitute an HTTPS bridge.
  Report server, launcher and
  actual Desktop execution evidence separately; parser tests alone do not prove
  that a task was created or that an inference request was sent.
- Desktop launcher policy (user-updated 2026-09-29): isolate only `auth.json` and
  `config.toml` per service. Share the original client's session home, SQLite,
  history, projects, plugins and Desktop data. This supersedes the earlier fully
  isolated profile policy. Preserve old isolated data; do not overwrite or merge
  live SQLite files. Initialize file credential
  storage and service addresses without copying default credentials or overwriting
  existing client preferences/tokens. Let the original client perform login and
  token refresh in that profile. Keep the installed native runtime and address
  routing hooks; do not copy runtime executables or wrap app-server with a shim.
  The only login option
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
  to copy. Follow the current capability boundaries in `docs/ARCHITECTURE.md`; do not invent usage, task/log
  history, official grants, billing transactions or external-service success.
- These requirements do not authorize unrelated feature expansion or a Codex
  baseline update. Implement each requested repair through its data and UI path.

- Match existing crate boundaries.
- Keep account isolation. Never share auth, installation_id, HTTP client, or session state.
- Persist identity; do not regenerate installation_id on re-login of the same ChatGPT account.
- Dynamic IDs (session/thread/turn/request) stay request-scoped.
- Write compiling Rust (edition 2024). Prefer `anyhow`/`thiserror`, `axum` 0.8, `sqlx` sqlite, `reqwest` rustls.
- After your crate is implemented, `cargo check -p <crate>` should pass.
