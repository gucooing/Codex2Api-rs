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
for baseline changes. Build and test both frontend exports and the Rust workspace; release packaging uses the release workflow.

## Process and crates

One Rust process runs three listeners: AI API (8080), administration (8081), and the user website/API (8082). Each router registers only its own surface. The administrator and user Next.js projects produce separate static exports embedded in the same executable. Production does not run a Node.js server.

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
| codex2api-user | User browser sessions, wallet purchases, own subscriptions/devices and OAuth consent |
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

User identities, standalone virtual accounts, supplier accounts and administrator sessions are separate entities. Existing virtual accounts are never converted to users. `users` owns browser credentials and an integer-cent USD wallet. `user_subscriptions` uniquely maps `(user_id, provider_id)` to a generated platform virtual identity; existing virtual account administration lists only standalone accounts. User subscription configuration and history reuse the established virtual-account persistence and execution services without exposing their supplier binding to users.

 A consumer's provider is fixed at creation; execution routes may bind
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

Execution supply is a provider-scoped tag pool. Suppliers may have multiple tags;
each consumer selects one tag and keeps a temporary supplier assignment. SQLite
serializes candidate counting and assignment in one write transaction. Confirmed supplier outages repair affected assignments. New or
unavailable assignments select the healthy member with the fewest total current
bindings across all tags; healthy assignments stay sticky. Manual selection is
restricted to available members of that pool. Removing a member clears affected
assignments; the next request selects another member. Referenced tags cannot be
deleted. Migration preserves old isolation with one migration tag per bound supplier.

Supplier tag definitions are managed on their own administrator page. Account
detail and list batch editing submit a complete checked tag set, limited to the
suppliers' provider. `POST /admin/api/suppliers/tags` requires `account_ids` and
`tag_ids`: replacement is atomic, an explicit empty set clears memberships, and
omitted fields cannot clear tags. Unselected suppliers and other account fields
remain unchanged. Only edited selections are submitted.

Supplier availability separates manual disablement, permanent credential rejection
and quota exhaustion. Ordinary request throttling remains a client retry concern.
The ChatGPT adapter follows
`codex-api/src/api_bridge.rs`, `sse/responses_error.rs` and
`login/src/auth/manager.rs` in the pinned reference: `usage_limit_reached` and known
quota/credit codes indicate exhaustion. `rate_limit_exceeded`, `slow_down` and
unclassified HTTP 429 are forwarded with their retry information; they never
change supplier health or binding. 403, network failures, policy errors
and 5xx never permanently invalidate credentials. Refresh rejection follows the
official permanent codes, including HTTP 400 `invalid_grant`; expired access tokens
are refreshed before abandoning their supplier. Observations carry credential
revisions so stale failures cannot disable newly replaced authorization.

Quota cooldowns persist in SQLite and expire automatically at the official reset or
Retry-After time. When timing is absent, a sixty-second probe cooldown is used;
it is not presented as an official quota reset. Cached main quota responses can
also record exhaustion; model-specific additional windows do not disable the whole
account. The normal quota cache remains in use, without requests on UI timer ticks.

HTTP Responses and Responses WebSocket internally try each eligible pool member
at most once for unrecoverable authorization or explicit quota exhaustion.
Authentication recovery remains bounded.
SSE and WS buffer the small pre-generation prelude so a rejected attempt does not
leak its error/response ID into the client's successful generation. Prices and RPM
admission are captured once; only the winning attempt's reported usage settles the
request. Exhausted pools return a service `supplier_pool_exhausted` error rather
than the supplier's login or quota error.

Once response output or a tool event has been released, the generation cannot be
replayed safely. Network interruption, malformed streams and post-output failures
retain structured errors and close behavior; subsequent requests select a healthy
member. Responses WS serializes queued generations, rechecks credentials and policy,
and can reconnect supply between generations without replacing the virtual device.
Its in-memory conversation input/output permits replay of known previous-response
references on a new supplier. An opaque reference from outside that connection
requires full client input (`supplier_context_required`); private supplier resources
are never transferred between owners. Realtime and supplier-owned cloud resources
retain their existing transport/resource contracts and cannot be replayed across
accounts once established.

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

Standalone virtual subscriptions are issued by administrators. User subscriptions can be granted by administrators or purchased with the user wallet. Grants, renewals, plan changes and
expiration update the same persisted entitlements used by client responses and
execution checks. Expiration removes paid benefits but does not disable login.
Unsubscribed users receive the platform Free plan automatically. Expired paid subscriptions resolve to the same Free plan for identity, model permission, quotas and execution. `platform_free_plans` identifies this administrable policy; Free plans cannot be deleted or changed to a paid tier. There are no paid-plan-specific free-access controls. Model access explicitly distinguishes
all enabled models, a selected list, and no models.

Management uses named fields and choices. Actual activity, requests, device
authorizations and resource operations are viewable as records; they cannot be
manufactured with configuration forms. Empty results are valid only after querying
an implemented data source and finding no records.

### Login identity and data boundaries

`accounts` is the single login-identity table: immutable ID and account_type
(admin/user), username, password hash, enabled state and timestamps.
`admin_users.account_id` references an administrator account; `users.id` references
a user account and owns profile, wallet and revision data. SQL constraints/triggers
reject cross-role profiles and identity/type changes. Usernames are unique within
each account type, and each login endpoint always specifies its own type.

`admin_identities` and `user_identities` join only the corresponding role.
`virtual_principals` resolves a managed platform identity through its user owner;
managed identities do not duplicate passwords or profile fields. Standalone
virtual accounts stay independent and are never converted into users.
The user HTTP crate receives `UserStore`, a narrow repository with no raw SQL,
supplier or administrator access. Response DTOs explicitly select user-visible
fields; common signing primitives do not create a shared HTTP login/data endpoint.

### User wallet and subscription transactions

User creation atomically provisions a stable Free platform identity for each configured platform. Paid expiration is evaluated on reads; the paid expiration and purchase records remain inspectable without rewriting history.

Wallet balance is `users.wallet_cents`, with a nonnegative integer constraint. It starts at zero. Identity forms and user APIs cannot set it. Administrators use `/admin/api/users/{id}/wallet-adjustments` with a signed-in administrator session, CSRF, expected user revision, idempotency key, signed integer-cent delta and optional reason. Missing, null and blank reasons normalize to null; nonempty reasons retain their length limit. A single immediate transaction applies the delta and appends a `system_adjustment` ledger entry containing the operator snapshot and before/after balances. Overdraw, unsafe amounts and stale revisions are rejected. Replays return the same entry without another balance change. Ledger updates/deletions are prohibited; historical purchase entries remain intact. The administrator-only `GET /admin/api/wallet-entries` reads all users' ledger entries with exact user ID, source, optional Unix-millisecond start/end bounds and pagination. The count and rows share a read transaction, timestamps are compared as instants, and ordering breaks time ties by insertion order. List DTOs add only the owner username/name to ledger fields, including the original order reference. This capability is not exposed through UserStore or the user listener. User wallet DTOs show the change as a system operation without administrator identity or private reason. There is no external payment integration; checkout offers the USD wallet. Plan sale price uses at most two decimal USD places, separately from model-priced inference budgets. Plans have a purchase duration and an administrator-only default supplier tag. `virtual_plans.allow_purchase` is the only paid-catalog publication switch. Published plans appear even without a price or default tag; an unknown price stays null and cannot be ordered as zero. Supplier execution configuration does not gate wallet checkout or payment. Closing purchases blocks user purchases/renewals, not administrator assignments or existing benefits. Free is supplied automatically and is not sold.

`POST /user/api/checkout/preview` computes an unpersisted, five-minute quote with plan, effective period, current-subscription credit, coupon discount and payable amount. It never creates an order, reserves a coupon or debits the wallet. A short-lived RS256 proof binds the authenticated owner, selection, pricing instant and quote digest to `checkout_preview` / `codex2api-checkout` under the user signing key. It cannot authenticate a web session or Codex client. No supplier configuration is in the proof or response.

Only explicit confirmation calls `POST /user/api/orders` with that proof. Inside `BEGIN IMMEDIATE`, it recomputes the signed quote at its original pricing instant, validates current plan/subscription/coupon revisions and capacity, and creates the order. The proof ID is the owner-scoped idempotency key. Coupon eligibility and remaining-subscription expiry can shorten the original deadline; confirmation never extends it. Client amounts and owner selectors are not accepted. Order views explicitly select business fields.

Payment is a separate `/orders/{id}/pay` operation. It checks ownership, enabled state, deadline, plan publication/revision and subscription revision. The debit, entitlement update, price-period snapshot, unique order-linked wallet entry and paid state commit together. Replaying a paid order returns its existing result. Competing orders cannot both settle from one subscription revision. Plan changes, closed sales, changed/disabled subscriptions and expiry have distinct messages; cancelled orders retain their reason. Insufficient balance leaves the order pending, with no debit or entitlement mutation. Infrastructure failures are logged privately and return a generic retry message without database or supplier detail. Paid orders are immutable.

Coupons are administrator-owned fixed-cent discounts with optional plan scope, minimum subtotal after subscription credit, validity dates, global capacity and a per-user limit. Previews do not reserve capacity. Confirmation reserves by creating a live pending order under the same write lock; cancellation/expiry release it, and payment consumes it. Confirmed orders keep the coupon code and discount snapshot despite later coupon edits or disabling. Coupon records cannot be deleted through the API. Discount is capped at the nonnegative subtotal; zero-price orders still require both confirmation and explicit payment. `amount_cents` is generated from the stored subtotal minus discount.

New purchases start their full duration on payment. Same-plan renewal extends from
the existing expiry and preserves the spending-window anchor. Active plan changes
compare the target daily sale price with the current period's original price snapshot;
only a strictly higher daily price is an upgrade. Lower/equal-price changes wait until
expiration, when an ordinary purchase starts a new period. Infinite subscriptions
cannot be prorated and require administrator management.

Upgrade quotes calculate the target daily price times remaining time, less the
remaining value of each originally granted/purchased/renewed price period. Integer
arithmetic rounds each invoice line half up to a USD cent; payable amounts cannot
be negative and do not create a refund/funding entry. The checkout shows gross cost,
credit, payable amount and quote expiry before confirmation. Upgrades keep the
original expiration and spending anchor, applying the target model/budget policy and
execution route. `subscription_pricing_periods` preserves nominal price and duration
snapshots, including administrator-issued periods; subsequent price edits cannot
reprice those credits. Nominal prices determine upgrade rank; separate immutable credit-value/duration snapshots preserve the value after coupon discounts. Coupons never increase future credit value. Renewed periods retain their own prices. Administrator grants
are not fabricated orders or wallet payments. Missing historical price snapshots or
unpriced grants are explicitly unavailable for proration, never treated as zero;
the administrator can explicitly reissue the subscription at the current price to
establish a new snapshot and spending anchor. Ordinary enable/disable edits preserve
the original pricing; this reissue action is separate and never debits a wallet. Existing wallet entries
remain historical records without invented orders.

User interfaces are `/checkout/preview`, `/orders` (list/confirm), `/orders/{id}` (detail), and
`/orders/{id}/pay` or `/cancel` under `/user/api`. Administration has its own read and
pending-cancellation endpoints under `/admin/api/orders`; no shared authentication
or user-supplied owner selector is used. Order lists are paginated and filtered by order number, plan ID, and status; administrator routes additionally accept an exact user ID. User routes reject owner selectors. `/orders/plans` returns only distinct plans present in that caller's order scope, retaining historical names after sales close or catalog removal. The retired direct-purchase endpoint is not mounted. Coupon management has its own administrator-only `/admin/api/coupons` endpoints. There is no user coupon-list or coupon-management interface.

User `/usage` statistics, daily aggregation and paginated records each restrict `subject_kind` and join subscription ownership to the authenticated user; query parameters cannot select a different owner or supplier. Explicit record fields exclude supplier IDs/names, upstream request IDs, raw errors, routing and credentials. The user dashboard shows current subscriptions, local spending windows and real usage trends. The usage detail page contains filters, numeric summaries and paginated records, without duplicate charts. Date bucketing groups the signed timezone offset as an SQL expression, including negative offsets; an empty ledger returns zero request counts and empty rows without errors. Empty dates are zero-filled only after a successful query; missing token/cost measurements stay unknown with missing counts. Model-priced USD costs consume subscription budgets, not the wallet. Codex client usage response schemas remain unchanged.

User subscription administration hides expired entries by default and offers an explicit include-expired filter. Configuration/records links expose the existing client-data administration tools. Identity and subscription edits belong in user/subscription management; direct standalone-account writes cannot alter managed user accounts. The existing standalone virtual-account module remains independent.

### Request admission and subscription names

RPM is a consumer-wide sliding sixty-second window across devices and HTTP/WS,
persisted in SQLite and admitted atomically. Gateway settings default to 20;
missing account configuration inherits it, an account override wins, and zero
means unlimited. Each actual generation consumes one admission; internal supplier
attempts, warmups, reads and WS handshakes do not. Handshakes check the current limit
without consuming it. Rejections use HTTP/WS 429 `virtual_rpm_exceeded` with retry
information and are distinct from the consumer's spending quota.

Plan management selects the official client subscription tier independently of the
business plan name. Updating that tier atomically updates assigned accounts while
preserving subscription dates and historical records. Choosing Free applies that plan's own model and spending-window configuration. Public tier names include
Free, Go, Plus, Pro 100, Pro 200, Pro 500, Business, Enterprise and Edu. Protocol
values remain canonical (`prolite`, `pro`, `promax`, etc.). Supplier display also
maps workspace/education variants to their official family name and does not expose
unknown internal values as product names. Numbered Pro labels follow the installed
Desktop's `plan-names` resource; accepted protocol variants follow the pinned
`protocol/src/account.rs`. Local plans never fabricate supplier entitlements.

### OAuth and devices

The AI API authorization entry redirects to the configured public user origin. Browser consent and device-code approval run only on the user listener. An authenticated user browser session selects that user's platform identity. Otherwise, an explicit user or standalone-virtual identity login verifies credentials for the current cookie-bound, CSRF-protected authorization flow only. The identity is displayed before a separate confirmation request. No code or device session is created by password verification alone.

Confirmation binds the displayed account, current password version, optional live browser session, state, exact callback and PKCE S256 challenge. Codes are single use. Browser logout invalidates unconfirmed browser-session grants; password changes or disablement invalidate the user's browser sessions, platform devices and pending authorizations. Standalone identities cannot log into the user website, and administrators authenticate only on the administrator listener. Expiration does not itself disable login.

Administrator and user **web sessions** use JWTs, with independent RSA keys in
`jwt_signing_keys`. Their endpoint selects the expected role/purpose and key domain.
Only the server's fixed RS256 algorithm is accepted; none, empty/other algorithms,
unsupported headers and unknown kid are rejected. Verification checks signature,
account_type, token_use, audience, issuer, lifetime and the live role/session record.
Web JWTs expire after 24 hours and cannot authorize AI/client endpoints. No opaque
web sessions or failed-Bearer-to-cookie fallback is accepted.

Codex OAuth belongs to the AI API protocol and retains its own SQLite-backed RSA
signer, official claim shape, token lifetimes, refresh/revoke contract and auxiliary
client credential formats. It does not gain web account_type/token_use fields or
web JWT rules. Client grants remain bound to the local platform account/device and
cannot authorize administration or user-management APIs. Only the browser consent
page uses the website's current user identity; it issues the ordinary PKCE-bound
OAuth authorization code after explicit confirmation. The account-identity migration
invalidates old website sessions while preserving Codex OAuth keys, devices and grants.

Access-token audience is an API audience array with granted scp values;
ID-token audience is the CLI client ID array and at_hash binds the actual access
token. Refresh preserves authentication time and session identity. The service
does not claim official signatures, MFA, payments or verified email that did not occur.

Device authorization stores short-lived pending codes and PKCE state in
`virtual_device_authorizations`. Browser approval uses the same identity-then-confirmation/CSRF boundary and completes a normal virtual-device session. The administrator's
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

Client disconnection does not cancel an accepted upstream request. HTTP workers
continue while waiting for headers, and detached response bodies continue through
the same usage parser and account-local transforms. WebSocket relays finish the
active upstream generation after a close frame, TCP loss or downstream write
failure; unsent queued generations are discarded. The final upstream outcome and
reported usage determine the record and charge. Normal upstream failures, idle
timeouts and credential revocation checks still apply. Disconnection itself does
not mark a platform failure or finalize a partial charge.

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

The single administrator defaults to admin/admin on first initialization.
The admin and user websites store their own purpose-bound JWT in an HttpOnly cookie.
`CODEX2API_PUBLIC_ADMIN_URL` declares the administrator browser origin. HTTPS
enables `Secure` on session creation and deletion, including password changes;
HTTP origins retain local development support. Client-supplied `Forwarded` and
`X-Forwarded-Proto` headers cannot change that policy.
Each interface accepts only its own session purpose, whether supplied by Cookie or
Bearer; an invalid/wrong-purpose Bearer never falls back to another cookie. Mutations
also require the corresponding session's CSRF token. User APIs never serialize
supplier records, execution routes or administrator DTOs. AI OAuth credentials authorize only their provider/account and existing client scopes. Public upstream responses filter identity/credential headers and metadata; error bodies preserve classification and retry fields while withholding supplier diagnostics.

`frontend/` and `frontend-user/` export independent static assets embedded by codex2api-web. Its build script verifies
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
paths, support WebSocket and provide TLS when required. CODEX2API_PUBLIC_API_URL declares
an externally reachable origin; it does not start a TLS server.
See [CCODEX_ENDPOINTS.md](CCODEX_ENDPOINTS.md) for the current route groups.

Desktop support provides authenticated feature configuration and a cache of
allowlisted public resources. It does not collect client telemetry, SDK exceptions,
Statsig events or metrics. Retired intake URLs return 404 without recording their
requests; there is no diagnostic management API or collection setting. Database
migration removes the collected diagnostic data while preserving public resources,
account-owned activity and the inference billing ledger.

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

### Administrator filters and overview counts

Order and subscription filters use exact plan/user IDs. `/admin/api/users/options` searches usernames, names and emails and returns at most five minimal choices; it is never mounted on the user listener. Subscription filters retain the default exclusion of expired records. Administrative usage filtering applies the chosen user to every count and record query through stable subscription ownership, including all of that user's platforms.

Overview virtual counts include only standalone virtual accounts; normal means enabled with no expiry or a future expiry. User totals include all user profiles. Daily active users are distinct users with any actual ledger record in the administrator browser's local day, including failed/in-progress requests, never device/session counts. The browser supplies its UTC offset; server-generated day boundaries exclude future records. Changing filters never calls a supplier.
