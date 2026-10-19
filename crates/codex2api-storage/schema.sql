-- Current PostgreSQL schema.

CREATE TABLE meta (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);

CREATE TABLE oauth_pending (
    state TEXT PRIMARY KEY NOT NULL,
    code_verifier TEXT NOT NULL,
    redirect_uri TEXT NOT NULL,
    account_id TEXT,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    flow_data_json TEXT NOT NULL DEFAULT '{}',
    last_polled_at_ms BIGINT
);

CREATE TABLE outbound_proxies (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    url TEXT NOT NULL,
    created_at TEXT NOT NULL,
    exit_ip TEXT,
    country_code TEXT,
    country TEXT,
    region TEXT,
    city TEXT,
    connection_ok BOOLEAN,
    connection_latency_ms BIGINT,
    connection_error TEXT,
    connection_checked_at TEXT,
    quality_ok BOOLEAN,
    quality_latency_ms BIGINT,
    quality_http_status BIGINT,
    quality_error TEXT,
    quality_checked_at TEXT,
    timezone TEXT
);

CREATE TABLE oauth_browser_flows (
    id TEXT PRIMARY KEY NOT NULL,
    cookie_hash TEXT NOT NULL,
    csrf_hash TEXT NOT NULL,
    request_json TEXT NOT NULL,
    expires_at BIGINT NOT NULL
);

CREATE TABLE oauth_missing_endpoints (
    method TEXT NOT NULL,
    path TEXT NOT NULL,
    hits BIGINT NOT NULL DEFAULT 1,
    first_seen_at TEXT NOT NULL,
    last_seen_at TEXT NOT NULL,
    PRIMARY KEY(method,path)
);

CREATE TABLE desktop_public_resources (
    path TEXT PRIMARY KEY,
    content BYTEA NOT NULL,
    headers_json TEXT NOT NULL,
    fetched_at_ms BIGINT NOT NULL
);

CREATE TABLE providers (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL
);

CREATE TABLE model_catalog (
    provider_id TEXT NOT NULL REFERENCES providers(id),
    model TEXT NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('text','image')),
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    deleted BOOLEAN NOT NULL DEFAULT FALSE,
    revision BIGINT NOT NULL DEFAULT 1,
    PRIMARY KEY(provider_id,model)
);

CREATE TABLE model_image_prices (
    provider_id TEXT NOT NULL,
    model TEXT NOT NULL,
    resolution TEXT NOT NULL,
    price_nano_usd BIGINT NOT NULL CHECK(price_nano_usd>=0),
    PRIMARY KEY(provider_id,model,resolution),
    FOREIGN KEY(provider_id,model) REFERENCES model_catalog(provider_id,model)
);

CREATE TABLE model_prices (
    provider_id TEXT NOT NULL REFERENCES providers(id),
    model TEXT NOT NULL,
    tier TEXT NOT NULL,
    min_input_tokens BIGINT NOT NULL DEFAULT 0,
    input_rate BIGINT NOT NULL CHECK(input_rate>=0),
    cached_rate BIGINT NOT NULL CHECK(cached_rate>=0),
    cache_write_rate BIGINT NOT NULL CHECK(cache_write_rate>=0),
    output_rate BIGINT NOT NULL CHECK(output_rate>=0),
    source TEXT NOT NULL,
    revision BIGINT NOT NULL DEFAULT 1,
    max_input_tokens BIGINT CHECK(max_input_tokens IS NULL OR max_input_tokens >= min_input_tokens),
    PRIMARY KEY(provider_id,model,tier,min_input_tokens)
);

CREATE TABLE admin_subscription_operations (
    scope TEXT NOT NULL,
    request_id TEXT NOT NULL,
    signature TEXT NOT NULL,
    result_json TEXT NOT NULL,
    created_at_ms BIGINT NOT NULL,
    PRIMARY KEY(scope, request_id)
);

CREATE TABLE supplier_accounts (
    id TEXT PRIMARY KEY NOT NULL,
    status TEXT NOT NULL,
    display_name TEXT,
    chatgpt_account_id TEXT,
    chatgpt_user_id TEXT,
    email TEXT,
    plan_type TEXT,
    installation_id TEXT NOT NULL,
    os_type TEXT NOT NULL,
    os_version TEXT NOT NULL,
    arch TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    last_used_at TEXT,
    http_fingerprint_json TEXT NOT NULL DEFAULT '{}',
    proxy_id TEXT REFERENCES outbound_proxies(id) ON DELETE RESTRICT,
    provider_id TEXT NOT NULL DEFAULT 'chatgpt',
    auth_revision BIGINT NOT NULL DEFAULT 0,
    CONSTRAINT supplier_account_identity UNIQUE (provider_id, chatgpt_account_id, chatgpt_user_id)
);

CREATE TABLE supplier_tags (
    id TEXT PRIMARY KEY,
    provider_id TEXT NOT NULL REFERENCES providers(id),
    name TEXT NOT NULL CHECK(length(trim(name)) BETWEEN 1 AND 80)
);

CREATE TABLE supplier_tag_members (
    tag_id TEXT NOT NULL REFERENCES supplier_tags(id) ON DELETE CASCADE,
    account_id TEXT NOT NULL REFERENCES supplier_accounts(id) ON DELETE CASCADE,
    PRIMARY KEY(tag_id,account_id)
);

CREATE TABLE accounts (
    id TEXT PRIMARY KEY NOT NULL,
    account_type TEXT NOT NULL CHECK(account_type IN ('admin','user')),
    username TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT TRUE CHECK(enabled IN (FALSE,TRUE)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE admin_users (
    id BIGINT PRIMARY KEY CHECK(id=1),
    account_id TEXT NOT NULL UNIQUE REFERENCES accounts(id) ON DELETE CASCADE
);

CREATE TABLE users (
    id TEXT PRIMARY KEY NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    email TEXT NOT NULL,
    wallet_cents BIGINT NOT NULL DEFAULT 0 CHECK(wallet_cents>=0),
    revision BIGINT NOT NULL DEFAULT 1,
    kind TEXT NOT NULL DEFAULT 'regular' CHECK(kind='regular')
);

CREATE TABLE jwt_signing_keys (
    account_type TEXT PRIMARY KEY NOT NULL CHECK(account_type IN ('admin','user')),
    private_key TEXT NOT NULL UNIQUE
);

CREATE TABLE virtual_plans (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    plan_type TEXT NOT NULL,
    config JSONB NOT NULL,
    allow_purchase BOOLEAN NOT NULL DEFAULT TRUE,
    revision BIGINT NOT NULL DEFAULT 0,
    updated_at_ms BIGINT NOT NULL DEFAULT 0,
    provider_id TEXT NOT NULL DEFAULT 'chatgpt' REFERENCES providers(id),
    CONSTRAINT virtual_plans_provider_tier CHECK((provider_id='chatgpt' AND plan_type IN ('free','go','plus','prolite','pro','promax','team','business','enterprise','edu','edu_plus','edu_pro','self_serve_business_prolite','self_serve_business_usage_based','ent26','enterprise_cbp_automation','enterprise_cbp_usage_based')) OR (provider_id='grok' AND plan_type IN ('free','x_basic','x_premium','x_premium_plus','supergrok_lite','supergrok','supergrok_plus','supergrok_heavy'))),
    sort_id BIGINT GENERATED ALWAYS AS IDENTITY UNIQUE
);

CREATE TABLE operation_prices (
    provider_id TEXT NOT NULL,
    operation TEXT NOT NULL,
    price_nano_usd BIGINT CHECK (price_nano_usd >= 0),
    revision BIGINT NOT NULL DEFAULT 1,
    PRIMARY KEY (provider_id, operation)
);

CREATE TABLE supplier_scheduling (
    account_id TEXT PRIMARY KEY REFERENCES supplier_accounts(id) ON DELETE CASCADE,
    concurrency BIGINT NOT NULL DEFAULT 5 CHECK(concurrency BETWEEN 0 AND 1000000),
    priority INTEGER NOT NULL DEFAULT 0 CHECK(priority BETWEEN 0 AND 1000000),
    load_factor BIGINT CHECK(load_factor BETWEEN 1 AND 1000000),
    revision BIGINT NOT NULL DEFAULT 1
);

CREATE TABLE supplier_models (
    account_id TEXT NOT NULL REFERENCES supplier_accounts(id) ON DELETE CASCADE,
    model TEXT NOT NULL,
    source TEXT NOT NULL CHECK(source IN ('upstream','custom')),
    descriptor_json TEXT NOT NULL DEFAULT '{}',
    observed_at TEXT,
    PRIMARY KEY(account_id,model)
);

CREATE TABLE supplier_model_sync (
    account_id TEXT PRIMARY KEY REFERENCES supplier_accounts(id) ON DELETE CASCADE,
    observed_at TEXT NOT NULL,
    auth_revision BIGINT NOT NULL
);

CREATE TABLE api_groups (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    provider_id TEXT NOT NULL REFERENCES providers(id),
    enabled BOOLEAN NOT NULL DEFAULT TRUE CHECK(enabled IN (FALSE,TRUE)),
    deleted BOOLEAN NOT NULL DEFAULT FALSE CHECK(deleted IN (FALSE,TRUE)),
    allowed_models JSONB NOT NULL DEFAULT '[]',
    multiplier_micros BIGINT NOT NULL DEFAULT 1000000 CHECK(multiplier_micros BETWEEN 0 AND 1000000000),
    concurrency_limit BIGINT NOT NULL DEFAULT 0 CHECK(concurrency_limit BETWEEN 0 AND 1000000),
    rpm_limit BIGINT NOT NULL DEFAULT 0 CHECK(rpm_limit BETWEEN 0 AND 1000000),
    model_rules JSONB NOT NULL DEFAULT '[]',
    revision BIGINT NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    search_price_nano_usd BIGINT CHECK(search_price_nano_usd IS NULL OR (provider_id='chatgpt' AND search_price_nano_usd>=0))
);

CREATE TABLE api_group_tags (
    group_id TEXT NOT NULL REFERENCES api_groups(id) ON DELETE CASCADE,
    tag_id TEXT NOT NULL REFERENCES supplier_tags(id) ON DELETE RESTRICT,
    PRIMARY KEY(group_id,tag_id)
);

CREATE TABLE api_keys (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    group_id TEXT NOT NULL REFERENCES api_groups(id) ON DELETE RESTRICT,
    name TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    key_prefix TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT TRUE CHECK(enabled IN (FALSE,TRUE)),
    revoked_at TEXT,
    created_at TEXT NOT NULL,
    last_used_at TEXT,
    revision BIGINT NOT NULL DEFAULT 1
);

CREATE TABLE usage_records (
    id TEXT PRIMARY KEY NOT NULL,
    account_id TEXT NOT NULL,
    account_name TEXT NOT NULL,
    subject_id TEXT NOT NULL,
    subject_name TEXT NOT NULL,
    endpoint TEXT NOT NULL,
    upstream_endpoint TEXT,
    transport TEXT NOT NULL,
    model TEXT,
    reasoning_effort TEXT,
    input_tokens BIGINT,
    output_tokens BIGINT,
    cached_tokens BIGINT,
    cache_write_tokens BIGINT,
    reasoning_tokens BIGINT,
    image_size TEXT,
    total_ms BIGINT,
    requested_at_ms BIGINT NOT NULL,
    status TEXT NOT NULL DEFAULT 'in_progress',
    http_status BIGINT,
    actual_model TEXT,
    service_tier TEXT,
    pricing_snapshot_json TEXT,
    cost_nano_usd BIGINT,
    billing_status TEXT NOT NULL DEFAULT 'pending',
    billing_model TEXT,
    image_count BIGINT,
    image_usage_json TEXT,
    subject_kind TEXT NOT NULL DEFAULT 'virtual_account' CHECK(subject_kind IN ('virtual_account','api_key')),
    provider_id TEXT NOT NULL DEFAULT 'chatgpt',
    error_code TEXT,
    error_message TEXT,
    upstream_request_id TEXT,
    quota_reset_credit_id TEXT,
    image_input_usage_json TEXT,
    billing_tier TEXT,
    user_id TEXT REFERENCES users(id) ON DELETE RESTRICT,
    key_id TEXT REFERENCES api_keys(id) ON DELETE RESTRICT,
    group_id TEXT REFERENCES api_groups(id) ON DELETE RESTRICT,
    group_name TEXT,
    wallet_user_id TEXT REFERENCES users(id) ON DELETE RESTRICT,
    multiplier_micros BIGINT NOT NULL DEFAULT 1000000 CHECK(multiplier_micros BETWEEN 0 AND 1000000000),
    base_cost_nano_usd BIGINT,
    wallet_debit_cents BIGINT NOT NULL DEFAULT 0 CHECK(wallet_debit_cents>=0),
    completed_at_ms BIGINT,
    stream BOOLEAN CHECK(stream IS NULL OR stream IN (FALSE,TRUE)),
    source_name TEXT,
    user_agent TEXT,
    client_ip TEXT CHECK(client_ip IS NULL OR length(client_ip) BETWEEN 2 AND 45),
    first_token_ms BIGINT,
    CHECK(cost_nano_usd IS NULL OR (cost_nano_usd>=0)),
    CHECK(wallet_user_id IS NULL OR (user_id IS NOT NULL AND wallet_user_id=user_id))
);

CREATE TABLE usage_wallet_state (
    user_id TEXT PRIMARY KEY NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    pending_nano_usd BIGINT NOT NULL DEFAULT 0 CHECK(pending_nano_usd>=0),
    last_usage_id TEXT REFERENCES usage_records(id) ON DELETE RESTRICT
);

CREATE TABLE platform_accounts (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    provider_id TEXT NOT NULL REFERENCES providers(id),
    plan_id TEXT NOT NULL REFERENCES virtual_plans(id) ON DELETE RESTRICT,
    plan_type TEXT NOT NULL,
    subscription_expires_at TEXT,
    subscription_started_at TEXT,
    quota_reset_credit_id TEXT,
    enabled BOOLEAN NOT NULL DEFAULT TRUE CHECK(enabled IN (FALSE,TRUE)),
    created_at TEXT NOT NULL,
    revision BIGINT NOT NULL DEFAULT 1
);

CREATE TABLE billing_cache_revisions (
    provider_id TEXT PRIMARY KEY,
    revision BIGINT NOT NULL DEFAULT 0
);

CREATE TABLE admin_sessions (
    id TEXT PRIMARY KEY NOT NULL,
    admin_user_id BIGINT NOT NULL REFERENCES admin_users(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    refresh_version BIGINT NOT NULL DEFAULT 0,
    refresh_issued_at BIGINT NOT NULL DEFAULT 0
);

CREATE TABLE supplier_tokens (
    account_id TEXT PRIMARY KEY NOT NULL REFERENCES supplier_accounts(id) ON DELETE CASCADE,
    auth_mode TEXT,
    id_token TEXT,
    access_token TEXT,
    refresh_token TEXT,
    last_refresh TEXT,
    raw_auth_json TEXT,
    updated_at TEXT NOT NULL
);

CREATE TABLE supplier_runtime (
    account_id TEXT PRIMARY KEY NOT NULL REFERENCES supplier_accounts(id) ON DELETE CASCADE,
    session_id TEXT,
    extra_json TEXT,
    updated_at TEXT NOT NULL
);

CREATE TABLE account_quota_cache (
    account_id TEXT PRIMARY KEY NOT NULL REFERENCES supplier_accounts(id) ON DELETE CASCADE,
    response_json TEXT NOT NULL,
    observed_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE virtual_devices (
    id TEXT PRIMARY KEY NOT NULL,
    virtual_account_id TEXT NOT NULL REFERENCES platform_accounts(id) ON DELETE CASCADE,
    refresh_hash TEXT NOT NULL UNIQUE,
    installation_id TEXT,
    user_agent TEXT NOT NULL,
    created_at TEXT NOT NULL,
    last_login_at TEXT NOT NULL,
    last_used_at TEXT,
    provider_id TEXT NOT NULL DEFAULT 'chatgpt',
    scopes TEXT NOT NULL DEFAULT 'openid profile email offline_access api.connectors.read api.connectors.invoke',
    authenticated_at_ms BIGINT,
    requested_at_ms BIGINT
);

CREATE TABLE virtual_access_tokens (
    token_hash TEXT PRIMARY KEY NOT NULL,
    device_id TEXT NOT NULL REFERENCES virtual_devices(id) ON DELETE CASCADE,
    expires_at BIGINT NOT NULL,
    scopes TEXT NOT NULL DEFAULT 'openid profile email offline_access api.connectors.read api.connectors.invoke'
);

CREATE TABLE virtual_authorization_codes (
    code_hash TEXT PRIMARY KEY NOT NULL,
    virtual_account_id TEXT NOT NULL REFERENCES platform_accounts(id) ON DELETE CASCADE,
    client_id TEXT NOT NULL,
    redirect_uri TEXT NOT NULL,
    code_challenge TEXT NOT NULL,
    expires_at BIGINT NOT NULL,
    provider_id TEXT NOT NULL DEFAULT 'chatgpt',
    scopes TEXT NOT NULL DEFAULT 'openid profile email offline_access api.connectors.read api.connectors.invoke',
    authenticated_at_ms BIGINT,
    requested_at_ms BIGINT,
    oidc_nonce TEXT
);

CREATE TABLE virtual_client_state (
    virtual_account_id TEXT NOT NULL REFERENCES platform_accounts(id) ON DELETE CASCADE,
    state_key TEXT NOT NULL,
    value_json JSONB NOT NULL,
    revision BIGINT NOT NULL DEFAULT 1,
    write_origin TEXT NOT NULL DEFAULT 'system',
    updated_at_ms BIGINT NOT NULL DEFAULT 0,
    PRIMARY KEY (virtual_account_id, state_key)
);

CREATE TABLE virtual_resources (
    virtual_account_id TEXT NOT NULL REFERENCES platform_accounts(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    id TEXT NOT NULL,
    upstream_account_id TEXT REFERENCES supplier_accounts(id) ON DELETE SET NULL,
    value_json JSONB NOT NULL,
    created_at_ms BIGINT NOT NULL,
    updated_at_ms BIGINT NOT NULL,
    PRIMARY KEY (virtual_account_id, kind, id)
);

CREATE TABLE virtual_events (
    id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    virtual_account_id TEXT NOT NULL REFERENCES platform_accounts(id) ON DELETE CASCADE,
    topic TEXT NOT NULL,
    value_json JSONB NOT NULL,
    created_at_ms BIGINT NOT NULL
);

CREATE TABLE virtual_request_logs (
    id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    virtual_account_id TEXT NOT NULL REFERENCES platform_accounts(id) ON DELETE CASCADE,
    device_id TEXT NOT NULL,
    method TEXT NOT NULL,
    path TEXT NOT NULL,
    status BIGINT NOT NULL,
    duration_ms BIGINT NOT NULL,
    created_at_ms BIGINT NOT NULL
);

CREATE TABLE virtual_conduits (
    token_hash TEXT PRIMARY KEY NOT NULL,
    virtual_account_id TEXT NOT NULL REFERENCES platform_accounts(id) ON DELETE CASCADE,
    upstream_account_id TEXT NOT NULL REFERENCES supplier_accounts(id) ON DELETE CASCADE,
    upstream_token TEXT NOT NULL,
    expires_at BIGINT NOT NULL
);

CREATE TABLE virtual_remote_servers (
    id TEXT PRIMARY KEY,
    environment_id TEXT NOT NULL UNIQUE,
    virtual_account_id TEXT NOT NULL REFERENCES platform_accounts(id) ON DELETE CASCADE,
    device_id TEXT NOT NULL REFERENCES virtual_devices(id) ON DELETE CASCADE,
    installation_id TEXT NOT NULL,
    name TEXT NOT NULL,
    os TEXT NOT NULL,
    arch TEXT NOT NULL,
    app_server_version TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    expires_at BIGINT NOT NULL,
    connection_id TEXT,
    connected_until_ms BIGINT NOT NULL DEFAULT 0,
    created_at_ms BIGINT NOT NULL,
    last_seen_at_ms BIGINT NOT NULL,
    UNIQUE(virtual_account_id, installation_id)
);

CREATE TABLE supplier_health (
    account_id TEXT PRIMARY KEY REFERENCES supplier_accounts(id) ON DELETE CASCADE,
    error_message TEXT,
    error_at TEXT,
    revision BIGINT NOT NULL DEFAULT 0,
    rejected_auth_revision BIGINT,
    cooldown_kind TEXT CHECK(cooldown_kind IN ('quota_exhausted')),
    cooldown_until BIGINT,
    cooldown_auth_revision BIGINT,
    cooldown_code TEXT,
    cooldown_observed_at BIGINT,
    cooldown_revision BIGINT NOT NULL DEFAULT 0,
    payment_required_at TEXT,
    payment_required_code TEXT
);

CREATE TABLE supplier_info_cache (
    account_id TEXT NOT NULL REFERENCES supplier_accounts(id) ON DELETE CASCADE,
    section TEXT NOT NULL CHECK (section IN ('usage', 'details', 'credits')),
    response_json TEXT NOT NULL,
    observed_at TIMESTAMPTZ NOT NULL,
    auth_revision BIGINT,
    PRIMARY KEY (account_id, section)
);

CREATE TABLE virtual_reset_grants (
    virtual_account_id TEXT NOT NULL REFERENCES platform_accounts(id) ON DELETE CASCADE,
    request_id TEXT NOT NULL,
    quantity BIGINT NOT NULL CHECK(quantity BETWEEN 1 AND 100),
    note TEXT NOT NULL,
    created_at_ms BIGINT NOT NULL,
    available_at_ms BIGINT NOT NULL,
    expires_at_ms BIGINT,
    PRIMARY KEY(virtual_account_id, request_id)
);

CREATE TABLE virtual_reset_credits (
    id TEXT PRIMARY KEY,
    virtual_account_id TEXT NOT NULL REFERENCES platform_accounts(id) ON DELETE CASCADE,
    grant_request_id TEXT NOT NULL,
    granted_at_ms BIGINT NOT NULL,
    redeemed_at_ms BIGINT,
    redeemed_by TEXT,
    windows_reset BIGINT NOT NULL DEFAULT 0,
    available_at_ms BIGINT NOT NULL DEFAULT 0,
    expires_at_ms BIGINT,
    source TEXT NOT NULL DEFAULT 'card',
    FOREIGN KEY(virtual_account_id, grant_request_id) REFERENCES virtual_reset_grants(virtual_account_id, request_id)
);

CREATE TABLE virtual_reset_requests (
    virtual_account_id TEXT NOT NULL REFERENCES platform_accounts(id) ON DELETE CASCADE,
    request_id TEXT NOT NULL,
    response_json TEXT NOT NULL,
    created_at_ms BIGINT NOT NULL,
    PRIMARY KEY(virtual_account_id, request_id)
);

CREATE TABLE virtual_device_authorizations (
    id_hash TEXT PRIMARY KEY,
    user_code_hash TEXT NOT NULL UNIQUE,
    client_id TEXT NOT NULL,
    redirect_uri TEXT NOT NULL,
    code_verifier TEXT NOT NULL,
    code_challenge TEXT NOT NULL,
    created_at BIGINT NOT NULL,
    expires_at BIGINT NOT NULL,
    last_poll_at BIGINT NOT NULL DEFAULT 0,
    authorization_code TEXT,
    virtual_account_id TEXT REFERENCES platform_accounts(id) ON DELETE CASCADE,
    provider_id TEXT NOT NULL DEFAULT 'chatgpt' REFERENCES providers(id),
    poll_interval BIGINT NOT NULL DEFAULT 5,
    requested_scopes TEXT
);

CREATE TABLE virtual_rpm_limits (
    virtual_account_id TEXT PRIMARY KEY REFERENCES platform_accounts(id) ON DELETE CASCADE,
    rpm BIGINT NOT NULL CHECK(rpm BETWEEN 0 AND 1000000)
);

CREATE TABLE user_sessions (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    csrf_token TEXT NOT NULL,
    expires_at BIGINT NOT NULL,
    refresh_version BIGINT NOT NULL DEFAULT 0,
    refresh_issued_at BIGINT NOT NULL DEFAULT 0
);

CREATE TABLE oauth_browser_identities (
    flow_id TEXT PRIMARY KEY NOT NULL REFERENCES oauth_browser_flows(id) ON DELETE CASCADE,
    virtual_account_id TEXT NOT NULL REFERENCES platform_accounts(id) ON DELETE CASCADE,
    password_hash TEXT NOT NULL,
    session_id TEXT REFERENCES user_sessions(id) ON DELETE CASCADE
);

CREATE TABLE coupons (
    id TEXT PRIMARY KEY NOT NULL,
    code TEXT NOT NULL,
    name TEXT NOT NULL,
    enabled BOOLEAN NOT NULL CHECK(enabled IN (FALSE,TRUE)),
    discount_cents BIGINT NOT NULL CHECK(discount_cents>0),
    minimum_cents BIGINT NOT NULL CHECK(minimum_cents>=0),
    plan_id TEXT REFERENCES virtual_plans(id) ON DELETE RESTRICT,
    starts_at_ms BIGINT NOT NULL,
    ends_at_ms BIGINT NOT NULL CHECK(ends_at_ms>starts_at_ms),
    max_uses BIGINT CHECK(max_uses>0),
    per_user_limit BIGINT NOT NULL CHECK(per_user_limit>0),
    revision BIGINT NOT NULL DEFAULT 1,
    created_at_ms BIGINT NOT NULL
);

CREATE TABLE execution_routes (
    virtual_account_id TEXT NOT NULL REFERENCES platform_accounts(id) ON DELETE CASCADE,
    provider_id TEXT NOT NULL REFERENCES providers(id),
    tag_id TEXT REFERENCES supplier_tags(id) ON DELETE RESTRICT,
    revision BIGINT NOT NULL DEFAULT 1,
    PRIMARY KEY(virtual_account_id,provider_id),
    UNIQUE(virtual_account_id)
);

CREATE TABLE virtual_concurrency_limits (
    virtual_account_id TEXT PRIMARY KEY REFERENCES platform_accounts(id) ON DELETE CASCADE,
    concurrency BIGINT NOT NULL CHECK(concurrency BETWEEN 0 AND 1000000)
);

CREATE TABLE oauth_browser_users(flow_id TEXT PRIMARY KEY NOT NULL REFERENCES oauth_browser_flows(id) ON DELETE CASCADE,user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,password_hash TEXT NOT NULL,session_id TEXT REFERENCES user_sessions(id) ON DELETE CASCADE);

CREATE TABLE subscription_orders (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    request_id TEXT NOT NULL,
    request_signature TEXT NOT NULL,
    provider_id TEXT NOT NULL REFERENCES providers(id),
    plan_id TEXT NOT NULL,
    plan_name TEXT NOT NULL,
    plan_revision BIGINT NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('purchase','renew','upgrade')),
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','paid','cancelled')),
    subscription_id TEXT,
    subscription_revision BIGINT,
    previous_plan_id TEXT,
    previous_plan_name TEXT,
    unit_price_cents BIGINT NOT NULL CHECK(unit_price_cents>=0),
    duration_days BIGINT NOT NULL CHECK(duration_days BETWEEN 1 AND 3650),
    period_start_ms BIGINT NOT NULL,
    period_end_ms BIGINT NOT NULL CHECK(period_end_ms>period_start_ms),
    gross_cents BIGINT NOT NULL CHECK(gross_cents>=0),
    credit_cents BIGINT NOT NULL CHECK(credit_cents>=0),
    subtotal_cents BIGINT NOT NULL CHECK(subtotal_cents>=0),
    pricing_json TEXT NOT NULL CHECK(pricing_json::jsonb IS NOT NULL),
    created_at_ms BIGINT NOT NULL,
    quote_expires_at_ms BIGINT NOT NULL,
    paid_at_ms BIGINT,
    cancelled_at_ms BIGINT,
    cancel_reason TEXT,
    balance_cents BIGINT,
    discount_cents BIGINT NOT NULL DEFAULT 0 CHECK(discount_cents>=0 AND discount_cents<=subtotal_cents),
    amount_cents BIGINT GENERATED ALWAYS AS (subtotal_cents-discount_cents) STORED,
    payment_method TEXT NOT NULL DEFAULT 'wallet' CHECK(payment_method='wallet'),
    coupon_id TEXT REFERENCES coupons(id) ON DELETE RESTRICT,
    coupon_code TEXT,
    UNIQUE(user_id,request_id),
    CHECK(subtotal_cents=GREATEST(0,gross_cents-credit_cents)),
    CHECK((status='paid')=(paid_at_ms IS NOT NULL AND balance_cents IS NOT NULL))
);

CREATE TABLE wallet_entries (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    request_id TEXT NOT NULL,
    signature TEXT NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('order_payment','system_adjustment','usage')),
    plan_id TEXT,
    plan_revision BIGINT,
    plan_name TEXT,
    provider_id TEXT,
    usage_record_id TEXT REFERENCES usage_records(id) ON DELETE RESTRICT,
    amount_cents BIGINT NOT NULL,
    balance_cents BIGINT NOT NULL CHECK(balance_cents>=0),
    balance_before_cents BIGINT GENERATED ALWAYS AS (balance_cents-amount_cents) STORED,
    duration_days BIGINT,
    expires_at TEXT,
    created_at TEXT NOT NULL,
    order_id TEXT REFERENCES subscription_orders(id) ON DELETE RESTRICT,
    operator_account_id TEXT REFERENCES accounts(id) ON DELETE RESTRICT,
    operator_name TEXT,
    reason TEXT,
    UNIQUE(user_id,request_id),
    CHECK(balance_before_cents>=0),
    CHECK((kind='order_payment' AND amount_cents<=0 AND plan_id IS NOT NULL AND plan_name IS NOT NULL AND operator_account_id IS NULL AND reason IS NULL) OR (kind='system_adjustment' AND amount_cents!=0 AND plan_id IS NULL AND order_id IS NULL AND operator_account_id IS NOT NULL AND operator_name IS NOT NULL AND (reason IS NULL OR length(trim(reason))>0)) OR (kind='usage' AND amount_cents<0 AND plan_id IS NULL AND order_id IS NULL AND operator_account_id IS NULL AND operator_name IS NULL AND reason IS NULL AND usage_record_id IS NOT NULL)),
    sort_id BIGINT GENERATED ALWAYS AS IDENTITY UNIQUE
);

CREATE TABLE subscription_pricing_periods (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    subscription_id TEXT NOT NULL,
    plan_id TEXT NOT NULL,
    starts_at_ms BIGINT NOT NULL,
    ends_at_ms BIGINT NOT NULL CHECK(ends_at_ms>starts_at_ms),
    unit_price_cents BIGINT CHECK(unit_price_cents>=0),
    duration_days BIGINT NOT NULL CHECK(duration_days BETWEEN 1 AND 3650),
    source TEXT NOT NULL CHECK(source IN ('admin','order')),
    order_id TEXT REFERENCES subscription_orders(id) ON DELETE RESTRICT,
    superseded_at_ms BIGINT,
    created_at_ms BIGINT NOT NULL,
    credit_value_cents BIGINT CHECK(credit_value_cents>=0),
    credit_duration_ms BIGINT NOT NULL DEFAULT 1 CHECK(credit_duration_ms>0),
    CHECK((source='order')=(order_id IS NOT NULL))
);

CREATE INDEX idx_admin_sessions_expires ON admin_sessions(expires_at);

CREATE INDEX idx_oauth_pending_expires ON oauth_pending(expires_at);

CREATE INDEX oauth_browser_expiry ON oauth_browser_flows(expires_at);

CREATE INDEX virtual_devices_owner ON virtual_devices(virtual_account_id);

CREATE INDEX virtual_access_expiry ON virtual_access_tokens(expires_at);

CREATE INDEX virtual_events_owner ON virtual_events(virtual_account_id,id);

CREATE INDEX virtual_logs_owner ON virtual_request_logs(virtual_account_id,id);

CREATE INDEX virtual_reset_credits_owner ON virtual_reset_credits(virtual_account_id, redeemed_at_ms, granted_at_ms);

CREATE INDEX virtual_device_authorizations_expiry ON virtual_device_authorizations(expires_at);

CREATE INDEX supplier_tag_account ON supplier_tag_members(account_id);

CREATE INDEX user_sessions_owner ON user_sessions(user_id);

CREATE INDEX subscription_orders_owner_time ON subscription_orders(user_id,created_at_ms DESC,id);

CREATE INDEX subscription_orders_status_time ON subscription_orders(status,created_at_ms DESC,id);

CREATE INDEX subscription_orders_coupon ON subscription_orders(coupon_id,user_id,status,quote_expires_at_ms);

CREATE INDEX execution_route_tag ON execution_routes(tag_id);

CREATE INDEX supplier_models_model ON supplier_models(model,account_id);

CREATE INDEX api_keys_owner ON api_keys(user_id,created_at DESC,id);

CREATE INDEX usage_records_time ON usage_records(requested_at_ms DESC,id DESC);

CREATE INDEX usage_records_account_time ON usage_records(account_id,requested_at_ms DESC);

CREATE INDEX usage_records_subject_time ON usage_records(subject_kind,subject_id,requested_at_ms DESC);

CREATE INDEX usage_records_model_time ON usage_records(model,requested_at_ms DESC);

CREATE INDEX usage_records_owner_time ON usage_records(user_id,requested_at_ms DESC,id);

CREATE INDEX usage_records_key_time ON usage_records(key_id,requested_at_ms DESC,id);

CREATE INDEX usage_records_group_time ON usage_records(group_id,requested_at_ms DESC,id);

CREATE INDEX usage_records_budget_window ON usage_records(subject_id,requested_at_ms,quota_reset_credit_id,cost_nano_usd);

CREATE INDEX usage_records_billing_totals ON usage_records(subject_id,cost_nano_usd,billing_status);

CREATE UNIQUE INDEX wallet_entry_order ON wallet_entries(order_id) WHERE order_id IS NOT NULL;

CREATE INDEX wallet_entries_owner_time ON wallet_entries(user_id,created_at DESC,id);

CREATE INDEX platform_accounts_user_provider ON platform_accounts(user_id,provider_id,id);

CREATE INDEX subscription_pricing_current ON subscription_pricing_periods(subscription_id,superseded_at_ms,starts_at_ms);

CREATE INDEX platform_accounts_plan ON platform_accounts(plan_id);

CREATE VIEW admin_identities AS
SELECT p.id,a.id AS account_id,a.username,a.password_hash,a.created_at,a.updated_at
FROM admin_users p JOIN accounts a ON a.id=p.account_id AND a.account_type='admin' AND a.enabled=TRUE;

CREATE VIEW user_identities AS
SELECT u.id,a.username,a.password_hash,u.name,u.email,a.enabled,u.wallet_cents,u.revision,a.created_at,u.kind
FROM users u JOIN accounts a ON a.id=u.id AND a.account_type='user';

CREATE VIEW platform_principals AS
SELECT p.id,p.user_id,p.provider_id,a.username,a.password_hash,u.name,u.email,u.kind,
       p.plan_id,p.plan_type,p.subscription_expires_at,p.created_at,p.revision,
       (p.enabled=TRUE AND a.enabled=TRUE) AS enabled
FROM platform_accounts p JOIN users u ON u.id=p.user_id
JOIN accounts a ON a.id=u.id AND a.account_type='user';

CREATE VIEW regular_users AS SELECT * FROM user_identities WHERE kind='regular';

CREATE VIEW regular_platforms AS SELECT * FROM platform_principals WHERE kind='regular';

CREATE VIEW regular_subscription_pools AS SELECT a.id AS virtual_account_id,a.provider_id,p.id AS plan_id,(p.config::jsonb #>> '{supplier_tag_id}') AS tag_id FROM regular_platforms a JOIN virtual_plans p ON p.id=a.plan_id;

CREATE VIEW effective_execution_routes AS SELECT r.virtual_account_id,r.provider_id,r.revision,COALESCE(r.tag_id,p.tag_id) AS tag_id FROM execution_routes r JOIN regular_subscription_pools p ON p.virtual_account_id=r.virtual_account_id;

CREATE UNIQUE INDEX accounts_username_folded ON accounts(account_type,lower(username));

CREATE UNIQUE INDEX supplier_tags_name_folded ON supplier_tags(provider_id,lower(name));

CREATE UNIQUE INDEX coupons_code_folded ON coupons(lower(code));

CREATE INDEX plans_supplier_tag ON virtual_plans ((config->>'supplier_tag_id'));

CREATE INDEX resources_lookup ON virtual_resources (virtual_account_id,kind,created_at_ms DESC,id);

CREATE INDEX usage_in_progress ON usage_records (requested_at_ms) WHERE status='in_progress';

CREATE INDEX subscription_expiry ON platform_accounts (subscription_expires_at) WHERE enabled;

CREATE FUNCTION device_provider_registered_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NOT EXISTS(SELECT 1 FROM providers p WHERE p.id=NEW.provider_id) THEN
RAISE EXCEPTION USING MESSAGE = 'unknown session provider', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER device_provider_registered BEFORE INSERT ON virtual_devices
FOR EACH ROW EXECUTE FUNCTION device_provider_registered_fn();

CREATE FUNCTION device_provider_immutable_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NEW.provider_id!=OLD.provider_id THEN
RAISE EXCEPTION USING MESSAGE = 'session provider is immutable', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER device_provider_immutable BEFORE UPDATE OF provider_id ON virtual_devices
FOR EACH ROW EXECUTE FUNCTION device_provider_immutable_fn();

CREATE FUNCTION authorization_provider_registered_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NOT EXISTS(SELECT 1 FROM providers p WHERE p.id=NEW.provider_id) THEN
RAISE EXCEPTION USING MESSAGE = 'unknown authorization provider', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER authorization_provider_registered BEFORE INSERT ON virtual_authorization_codes
FOR EACH ROW EXECUTE FUNCTION authorization_provider_registered_fn();

CREATE FUNCTION supplier_provider_registered_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NOT EXISTS(SELECT 1 FROM providers p WHERE p.id=NEW.provider_id) THEN
RAISE EXCEPTION USING MESSAGE = 'unknown supplier provider', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER supplier_provider_registered BEFORE INSERT ON supplier_accounts
FOR EACH ROW EXECUTE FUNCTION supplier_provider_registered_fn();

CREATE FUNCTION supplier_provider_immutable_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NEW.provider_id!=OLD.provider_id THEN
RAISE EXCEPTION USING MESSAGE = 'supplier provider is immutable', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER supplier_provider_immutable BEFORE UPDATE OF provider_id ON supplier_accounts
FOR EACH ROW EXECUTE FUNCTION supplier_provider_immutable_fn();

CREATE FUNCTION supplier_workspace_identity_update_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NEW.chatgpt_account_id IS DISTINCT FROM OLD.chatgpt_account_id OR NEW.chatgpt_user_id IS DISTINCT FROM OLD.chatgpt_user_id THEN
UPDATE supplier_accounts SET auth_revision=auth_revision+1 WHERE id=NEW.id;
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER supplier_workspace_identity_update AFTER UPDATE OF chatgpt_account_id,chatgpt_user_id ON supplier_accounts
FOR EACH ROW EXECUTE FUNCTION supplier_workspace_identity_update_fn();

CREATE FUNCTION supplier_auth_insert_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
UPDATE supplier_accounts SET auth_revision=auth_revision+1 WHERE id=NEW.account_id;
RETURN NEW;
END;
$$;
CREATE TRIGGER supplier_auth_insert AFTER INSERT ON supplier_tokens
FOR EACH ROW EXECUTE FUNCTION supplier_auth_insert_fn();

CREATE FUNCTION supplier_auth_update_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NEW.access_token IS DISTINCT FROM OLD.access_token OR NEW.id_token IS DISTINCT FROM OLD.id_token
  OR NEW.refresh_token IS DISTINCT FROM OLD.refresh_token OR NEW.raw_auth_json IS DISTINCT FROM OLD.raw_auth_json THEN
UPDATE supplier_accounts SET auth_revision=auth_revision+1 WHERE id=NEW.account_id;
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER supplier_auth_update AFTER UPDATE ON supplier_tokens
FOR EACH ROW EXECUTE FUNCTION supplier_auth_update_fn();

CREATE FUNCTION supplier_auth_delete_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
UPDATE supplier_accounts SET auth_revision=auth_revision+1 WHERE id=OLD.account_id;
RETURN OLD;
END;
$$;
CREATE TRIGGER supplier_auth_delete AFTER DELETE ON supplier_tokens
FOR EACH ROW EXECUTE FUNCTION supplier_auth_delete_fn();

CREATE FUNCTION supplier_tag_provider_insert_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NOT EXISTS(SELECT 1 FROM supplier_tags t JOIN supplier_accounts a ON a.id=NEW.account_id WHERE t.id=NEW.tag_id AND t.provider_id=a.provider_id) THEN
RAISE EXCEPTION USING MESSAGE = 'supplier tag provider mismatch', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER supplier_tag_provider_insert BEFORE INSERT ON supplier_tag_members
FOR EACH ROW EXECUTE FUNCTION supplier_tag_provider_insert_fn();

CREATE FUNCTION supplier_tag_provider_immutable_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NEW.provider_id!=OLD.provider_id THEN
RAISE EXCEPTION USING MESSAGE = 'supplier tag provider is immutable', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER supplier_tag_provider_immutable BEFORE UPDATE OF provider_id ON supplier_tags
FOR EACH ROW EXECUTE FUNCTION supplier_tag_provider_immutable_fn();

CREATE FUNCTION account_type_immutable_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NEW.id!=OLD.id OR NEW.account_type!=OLD.account_type THEN
RAISE EXCEPTION USING MESSAGE = 'account identity and type are immutable', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER account_type_immutable BEFORE UPDATE OF id,account_type ON accounts
FOR EACH ROW EXECUTE FUNCTION account_type_immutable_fn();

CREATE FUNCTION admin_profile_type_insert_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NOT EXISTS(SELECT 1 FROM accounts WHERE id=NEW.account_id AND account_type='admin') THEN
RAISE EXCEPTION USING MESSAGE = 'administrator profile requires administrator account', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER admin_profile_type_insert BEFORE INSERT ON admin_users
FOR EACH ROW EXECUTE FUNCTION admin_profile_type_insert_fn();

CREATE FUNCTION admin_profile_owner_immutable_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NEW.account_id!=OLD.account_id THEN
RAISE EXCEPTION USING MESSAGE = 'administrator profile owner is immutable', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER admin_profile_owner_immutable BEFORE UPDATE OF account_id ON admin_users
FOR EACH ROW EXECUTE FUNCTION admin_profile_owner_immutable_fn();

CREATE FUNCTION user_profile_type_insert_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NOT EXISTS(SELECT 1 FROM accounts WHERE id=NEW.id AND account_type='user') THEN
RAISE EXCEPTION USING MESSAGE = 'user profile requires user account', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER user_profile_type_insert BEFORE INSERT ON users
FOR EACH ROW EXECUTE FUNCTION user_profile_type_insert_fn();

CREATE FUNCTION user_profile_owner_immutable_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NEW.id!=OLD.id THEN
RAISE EXCEPTION USING MESSAGE = 'user profile owner is immutable', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER user_profile_owner_immutable BEFORE UPDATE OF id ON users
FOR EACH ROW EXECUTE FUNCTION user_profile_owner_immutable_fn();

CREATE FUNCTION paid_order_immutable_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF OLD.status='paid' THEN
RAISE EXCEPTION USING MESSAGE = 'paid order is immutable', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER paid_order_immutable BEFORE UPDATE ON subscription_orders
FOR EACH ROW EXECUTE FUNCTION paid_order_immutable_fn();

CREATE FUNCTION subscription_price_immutable_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
RAISE EXCEPTION USING MESSAGE = 'subscription price snapshot is immutable', ERRCODE = '23514';
RETURN NEW;
END;
$$;
CREATE TRIGGER subscription_price_immutable BEFORE UPDATE OF unit_price_cents,duration_days,source,order_id ON subscription_pricing_periods
FOR EACH ROW EXECUTE FUNCTION subscription_price_immutable_fn();

CREATE FUNCTION subscription_credit_immutable_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
RAISE EXCEPTION USING MESSAGE = 'subscription credit snapshot is immutable', ERRCODE = '23514';
RETURN NEW;
END;
$$;
CREATE TRIGGER subscription_credit_immutable BEFORE UPDATE OF credit_value_cents,credit_duration_ms ON subscription_pricing_periods
FOR EACH ROW EXECUTE FUNCTION subscription_credit_immutable_fn();

CREATE FUNCTION plan_provider_immutable_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NEW.provider_id!=OLD.provider_id THEN
RAISE EXCEPTION USING MESSAGE = 'plan provider is immutable', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER plan_provider_immutable BEFORE UPDATE OF provider_id ON virtual_plans
FOR EACH ROW EXECUTE FUNCTION plan_provider_immutable_fn();

CREATE FUNCTION user_kind_immutable_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NEW.kind!=OLD.kind THEN
RAISE EXCEPTION USING MESSAGE = 'user kind is immutable', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER user_kind_immutable BEFORE UPDATE OF kind ON users
FOR EACH ROW EXECUTE FUNCTION user_kind_immutable_fn();

CREATE FUNCTION regular_user_session_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NOT EXISTS(SELECT 1 FROM user_identities WHERE id=NEW.user_id AND kind='regular' AND enabled=TRUE) THEN
RAISE EXCEPTION USING MESSAGE = 'user website requires a regular user', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER regular_user_session BEFORE INSERT ON user_sessions
FOR EACH ROW EXECUTE FUNCTION regular_user_session_fn();

CREATE FUNCTION user_session_owner_immutable_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NEW.user_id!=OLD.user_id THEN
RAISE EXCEPTION USING MESSAGE = 'session owner is immutable', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER user_session_owner_immutable BEFORE UPDATE OF user_id ON user_sessions
FOR EACH ROW EXECUTE FUNCTION user_session_owner_immutable_fn();

CREATE FUNCTION regular_subscription_order_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NOT EXISTS(SELECT 1 FROM regular_users WHERE id=NEW.user_id) THEN
RAISE EXCEPTION USING MESSAGE = 'orders require a regular user', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER regular_subscription_order BEFORE INSERT ON subscription_orders
FOR EACH ROW EXECUTE FUNCTION regular_subscription_order_fn();

CREATE FUNCTION subscription_order_owner_immutable_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NEW.user_id!=OLD.user_id THEN
RAISE EXCEPTION USING MESSAGE = 'order owner is immutable', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER subscription_order_owner_immutable BEFORE UPDATE OF user_id ON subscription_orders
FOR EACH ROW EXECUTE FUNCTION subscription_order_owner_immutable_fn();

CREATE FUNCTION platform_owner_immutable_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NEW.id!=OLD.id OR NEW.user_id!=OLD.user_id OR NEW.provider_id!=OLD.provider_id THEN
RAISE EXCEPTION USING MESSAGE = 'platform identity and owner are immutable', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER platform_owner_immutable BEFORE UPDATE OF id,user_id,provider_id ON platform_accounts
FOR EACH ROW EXECUTE FUNCTION platform_owner_immutable_fn();

CREATE FUNCTION platform_plan_insert_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NOT EXISTS(SELECT 1 FROM virtual_plans p WHERE p.id=NEW.plan_id AND p.provider_id=NEW.provider_id AND p.plan_type=NEW.plan_type) THEN
RAISE EXCEPTION USING MESSAGE = 'platform plan mismatch', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER platform_plan_insert BEFORE INSERT ON platform_accounts
FOR EACH ROW EXECUTE FUNCTION platform_plan_insert_fn();

CREATE FUNCTION platform_plan_update_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NOT EXISTS(SELECT 1 FROM virtual_plans p WHERE p.id=NEW.plan_id AND p.provider_id=NEW.provider_id AND p.plan_type=NEW.plan_type) THEN
RAISE EXCEPTION USING MESSAGE = 'platform plan mismatch', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER platform_plan_update BEFORE UPDATE OF plan_id,plan_type ON platform_accounts
FOR EACH ROW EXECUTE FUNCTION platform_plan_update_fn();

CREATE FUNCTION platform_revision_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
UPDATE platform_accounts SET revision=revision+1 WHERE id=NEW.id;
RETURN NEW;
END;
$$;
CREATE TRIGGER platform_revision AFTER UPDATE OF plan_id,plan_type,subscription_expires_at,enabled ON platform_accounts
FOR EACH ROW EXECUTE FUNCTION platform_revision_fn();

CREATE FUNCTION platform_disabled_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NEW.enabled=FALSE THEN
DELETE FROM virtual_devices WHERE virtual_account_id=NEW.id;
    DELETE FROM virtual_authorization_codes WHERE virtual_account_id=NEW.id;
    DELETE FROM virtual_device_authorizations WHERE virtual_account_id=NEW.id;
    DELETE FROM oauth_browser_identities WHERE virtual_account_id=NEW.id;
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER platform_disabled AFTER UPDATE OF enabled ON platform_accounts
FOR EACH ROW EXECUTE FUNCTION platform_disabled_fn();

CREATE FUNCTION account_credentials_changed_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NEW.password_hash!=OLD.password_hash OR NEW.enabled!=OLD.enabled THEN
DELETE FROM admin_sessions WHERE admin_user_id IN(SELECT id FROM admin_users WHERE account_id=NEW.id);
    DELETE FROM user_sessions WHERE user_id=NEW.id;
    DELETE FROM virtual_devices WHERE virtual_account_id IN(SELECT id FROM platform_accounts WHERE user_id=NEW.id);
    DELETE FROM virtual_authorization_codes WHERE virtual_account_id IN(SELECT id FROM platform_accounts WHERE user_id=NEW.id);
    DELETE FROM virtual_device_authorizations WHERE virtual_account_id IN(SELECT id FROM platform_accounts WHERE user_id=NEW.id);
    DELETE FROM oauth_browser_identities WHERE virtual_account_id IN(SELECT id FROM platform_accounts WHERE user_id=NEW.id);
    DELETE FROM oauth_browser_users WHERE user_id=NEW.id;
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER account_credentials_changed AFTER UPDATE OF password_hash,enabled ON accounts
FOR EACH ROW EXECUTE FUNCTION account_credentials_changed_fn();

CREATE FUNCTION user_device_enabled_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NOT EXISTS(SELECT 1 FROM platform_principals WHERE id=NEW.virtual_account_id AND enabled=TRUE AND provider_id=NEW.provider_id) THEN
RAISE EXCEPTION USING MESSAGE = 'account unavailable', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER user_device_enabled BEFORE INSERT ON virtual_devices
FOR EACH ROW EXECUTE FUNCTION user_device_enabled_fn();

CREATE FUNCTION user_authorization_enabled_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NOT EXISTS(SELECT 1 FROM platform_principals WHERE id=NEW.virtual_account_id AND enabled=TRUE AND provider_id=NEW.provider_id) THEN
RAISE EXCEPTION USING MESSAGE = 'account unavailable', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER user_authorization_enabled BEFORE INSERT ON virtual_authorization_codes
FOR EACH ROW EXECUTE FUNCTION user_authorization_enabled_fn();

CREATE FUNCTION execution_route_provider_insert_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NOT EXISTS(SELECT 1 FROM platform_accounts p WHERE p.id=NEW.virtual_account_id AND p.provider_id=NEW.provider_id)
 OR (NEW.tag_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM supplier_tags WHERE id=NEW.tag_id AND provider_id=NEW.provider_id)) THEN
RAISE EXCEPTION USING MESSAGE = 'execution route pool mismatch', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER execution_route_provider_insert BEFORE INSERT ON execution_routes
FOR EACH ROW EXECUTE FUNCTION execution_route_provider_insert_fn();

CREATE FUNCTION execution_route_provider_update_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NEW.virtual_account_id!=OLD.virtual_account_id OR NEW.provider_id!=OLD.provider_id
 OR (NEW.tag_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM supplier_tags WHERE id=NEW.tag_id AND provider_id=NEW.provider_id)) THEN
RAISE EXCEPTION USING MESSAGE = 'execution route pool mismatch', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER execution_route_provider_update BEFORE UPDATE ON execution_routes
FOR EACH ROW EXECUTE FUNCTION execution_route_provider_update_fn();

CREATE FUNCTION api_group_provider_immutable_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NEW.provider_id!=OLD.provider_id THEN
RAISE EXCEPTION USING MESSAGE = 'API group provider is immutable', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER api_group_provider_immutable BEFORE UPDATE OF provider_id ON api_groups
FOR EACH ROW EXECUTE FUNCTION api_group_provider_immutable_fn();

CREATE FUNCTION api_group_tag_provider_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NOT EXISTS(SELECT 1 FROM api_groups g JOIN supplier_tags t ON t.provider_id=g.provider_id WHERE g.id=NEW.group_id AND t.id=NEW.tag_id) THEN
RAISE EXCEPTION USING MESSAGE = 'API group tag provider mismatch', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER api_group_tag_provider BEFORE INSERT ON api_group_tags
FOR EACH ROW EXECUTE FUNCTION api_group_tag_provider_fn();

CREATE FUNCTION api_group_tag_immutable_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
RAISE EXCEPTION USING MESSAGE = 'API group tags must be replaced', ERRCODE = '23514';
RETURN NEW;
END;
$$;
CREATE TRIGGER api_group_tag_immutable BEFORE UPDATE ON api_group_tags
FOR EACH ROW EXECUTE FUNCTION api_group_tag_immutable_fn();

CREATE FUNCTION api_key_regular_user_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NOT EXISTS(SELECT 1 FROM regular_users WHERE id=NEW.user_id AND enabled=TRUE)
 OR NOT EXISTS(SELECT 1 FROM api_groups WHERE id=NEW.group_id AND enabled=TRUE AND deleted=FALSE) THEN
RAISE EXCEPTION USING MESSAGE = 'API keys require an enabled regular user and API group', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER api_key_regular_user BEFORE INSERT ON api_keys
FOR EACH ROW EXECUTE FUNCTION api_key_regular_user_fn();

CREATE FUNCTION api_key_identity_immutable_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NEW.id!=OLD.id OR NEW.user_id!=OLD.user_id OR NEW.group_id!=OLD.group_id
 OR NEW.token_hash!=OLD.token_hash OR NEW.key_prefix!=OLD.key_prefix
 OR (OLD.revoked_at IS NOT NULL AND (NEW.revoked_at IS NULL OR NEW.revoked_at!=OLD.revoked_at OR NEW.enabled=TRUE)) THEN
RAISE EXCEPTION USING MESSAGE = 'API key identity and revocation are immutable', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER api_key_identity_immutable BEFORE UPDATE ON api_keys
FOR EACH ROW EXECUTE FUNCTION api_key_identity_immutable_fn();

CREATE FUNCTION usage_owner_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF (NEW.user_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM regular_users WHERE id=NEW.user_id))
 OR (NEW.subject_kind='api_key' AND NOT EXISTS(SELECT 1 FROM api_keys k JOIN api_groups g ON g.id=k.group_id
     WHERE k.id=NEW.key_id AND k.id=NEW.subject_id AND k.user_id=NEW.user_id AND g.id=NEW.group_id AND g.provider_id=NEW.provider_id))
 OR (NEW.subject_kind='virtual_account' AND NEW.user_id IS NOT NULL AND NOT EXISTS(
     SELECT 1 FROM regular_platforms WHERE id=NEW.subject_id AND user_id=NEW.user_id AND provider_id=NEW.provider_id)) THEN
RAISE EXCEPTION USING MESSAGE = 'usage ownership mismatch', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER usage_owner BEFORE INSERT ON usage_records
FOR EACH ROW EXECUTE FUNCTION usage_owner_fn();

CREATE FUNCTION usage_identity_immutable_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF OLD.status!='in_progress' OR NEW.id!=OLD.id OR NEW.user_id IS DISTINCT FROM OLD.user_id
 OR NEW.subject_id!=OLD.subject_id OR NEW.subject_kind!=OLD.subject_kind OR NEW.key_id IS DISTINCT FROM OLD.key_id
 OR NEW.group_id IS DISTINCT FROM OLD.group_id OR NEW.group_name IS DISTINCT FROM OLD.group_name
 OR NEW.wallet_user_id IS DISTINCT FROM OLD.wallet_user_id OR NEW.provider_id!=OLD.provider_id OR NEW.model IS DISTINCT FROM OLD.model
 OR NEW.multiplier_micros!=OLD.multiplier_micros OR NEW.pricing_snapshot_json IS DISTINCT FROM OLD.pricing_snapshot_json
 OR NEW.requested_at_ms!=OLD.requested_at_ms OR NEW.endpoint!=OLD.endpoint THEN
RAISE EXCEPTION USING MESSAGE = 'usage identity and settled records are immutable', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER usage_identity_immutable BEFORE UPDATE ON usage_records
FOR EACH ROW EXECUTE FUNCTION usage_identity_immutable_fn();

CREATE FUNCTION usage_no_delete_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
RAISE EXCEPTION USING MESSAGE = 'usage records cannot be deleted', ERRCODE = '23514';
RETURN OLD;
END;
$$;
CREATE TRIGGER usage_no_delete BEFORE DELETE ON usage_records
FOR EACH ROW EXECUTE FUNCTION usage_no_delete_fn();

CREATE FUNCTION wallet_entry_immutable_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
RAISE EXCEPTION USING MESSAGE = 'wallet ledger entries are immutable', ERRCODE = '23514';
RETURN NEW;
END;
$$;
CREATE TRIGGER wallet_entry_immutable BEFORE UPDATE ON wallet_entries
FOR EACH ROW EXECUTE FUNCTION wallet_entry_immutable_fn();

CREATE FUNCTION wallet_entry_no_delete_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
RAISE EXCEPTION USING MESSAGE = 'wallet ledger entries cannot be deleted', ERRCODE = '23514';
RETURN OLD;
END;
$$;
CREATE TRIGGER wallet_entry_no_delete BEFORE DELETE ON wallet_entries
FOR EACH ROW EXECUTE FUNCTION wallet_entry_no_delete_fn();

CREATE FUNCTION regular_wallet_entry_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NOT EXISTS(SELECT 1 FROM regular_users WHERE id=NEW.user_id) THEN
RAISE EXCEPTION USING MESSAGE = 'wallet entries require a regular user', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER regular_wallet_entry BEFORE INSERT ON wallet_entries
FOR EACH ROW EXECUTE FUNCTION regular_wallet_entry_fn();

CREATE FUNCTION usage_wallet_entry_owner_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NEW.kind='usage' AND NOT EXISTS(SELECT 1 FROM usage_records WHERE id=NEW.usage_record_id AND wallet_user_id=NEW.user_id) THEN
RAISE EXCEPTION USING MESSAGE = 'usage wallet entry ownership mismatch', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER usage_wallet_entry_owner BEFORE INSERT ON wallet_entries
FOR EACH ROW EXECUTE FUNCTION usage_wallet_entry_owner_fn();

CREATE FUNCTION usage_wallet_regular_user_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NOT EXISTS(SELECT 1 FROM regular_users WHERE id=NEW.user_id) THEN
RAISE EXCEPTION USING MESSAGE = 'usage wallet requires a regular user', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER usage_wallet_regular_user BEFORE INSERT ON usage_wallet_state
FOR EACH ROW EXECUTE FUNCTION usage_wallet_regular_user_fn();

CREATE FUNCTION usage_wallet_owner_immutable_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NEW.user_id!=OLD.user_id THEN
RAISE EXCEPTION USING MESSAGE = 'usage wallet owner is immutable', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER usage_wallet_owner_immutable BEFORE UPDATE OF user_id ON usage_wallet_state
FOR EACH ROW EXECUTE FUNCTION usage_wallet_owner_immutable_fn();

CREATE FUNCTION usage_stream_immutable_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NEW.stream IS DISTINCT FROM OLD.stream THEN
RAISE EXCEPTION USING MESSAGE = 'usage response mode is immutable', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER usage_stream_immutable BEFORE UPDATE OF stream ON usage_records
FOR EACH ROW EXECUTE FUNCTION usage_stream_immutable_fn();

CREATE FUNCTION usage_source_name_immutable_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NEW.source_name IS DISTINCT FROM OLD.source_name THEN
RAISE EXCEPTION USING MESSAGE = 'usage source name is immutable', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER usage_source_name_immutable BEFORE UPDATE OF source_name ON usage_records
FOR EACH ROW EXECUTE FUNCTION usage_source_name_immutable_fn();

CREATE FUNCTION user_regular_only_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NEW.kind!='regular' THEN
RAISE EXCEPTION USING MESSAGE = 'only ordinary users are supported', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER user_regular_only BEFORE INSERT ON users
FOR EACH ROW EXECUTE FUNCTION user_regular_only_fn();

CREATE FUNCTION subscription_price_owner_immutable_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NEW.user_id!=OLD.user_id OR NEW.subscription_id!=OLD.subscription_id THEN
RAISE EXCEPTION USING MESSAGE = 'subscription history ownership is immutable', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER subscription_price_owner_immutable BEFORE UPDATE OF user_id,subscription_id ON subscription_pricing_periods
FOR EACH ROW EXECUTE FUNCTION subscription_price_owner_immutable_fn();

CREATE FUNCTION oauth_browser_subscription_owner_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NOT EXISTS(SELECT 1 FROM oauth_browser_users u JOIN platform_accounts a ON a.user_id=u.user_id WHERE u.flow_id=NEW.flow_id AND a.id=NEW.virtual_account_id) THEN
RAISE EXCEPTION USING MESSAGE = 'authorization subscription ownership mismatch', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER oauth_browser_subscription_owner BEFORE INSERT ON oauth_browser_identities
FOR EACH ROW EXECUTE FUNCTION oauth_browser_subscription_owner_fn();

CREATE FUNCTION oauth_browser_subscription_owner_update_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NOT EXISTS(SELECT 1 FROM oauth_browser_users u JOIN platform_accounts a ON a.user_id=u.user_id WHERE u.flow_id=NEW.flow_id AND a.id=NEW.virtual_account_id AND u.password_hash=NEW.password_hash) THEN
RAISE EXCEPTION USING MESSAGE = 'authorization subscription ownership mismatch', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER oauth_browser_subscription_owner_update BEFORE UPDATE ON oauth_browser_identities
FOR EACH ROW EXECUTE FUNCTION oauth_browser_subscription_owner_update_fn();

CREATE FUNCTION subscription_price_owner_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NOT EXISTS(SELECT 1 FROM regular_platforms WHERE id=NEW.subscription_id AND user_id=NEW.user_id) THEN
RAISE EXCEPTION USING MESSAGE = 'subscription price ownership mismatch', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER subscription_price_owner BEFORE INSERT ON subscription_pricing_periods
FOR EACH ROW EXECUTE FUNCTION subscription_price_owner_fn();

CREATE FUNCTION billing_catalog_changed() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE provider text;
BEGIN
    provider := CASE WHEN TG_OP = 'DELETE' THEN OLD.provider_id ELSE NEW.provider_id END;
    INSERT INTO billing_cache_revisions(provider_id,revision) VALUES(provider,1)
    ON CONFLICT(provider_id) DO UPDATE SET revision=billing_cache_revisions.revision+1;
    RETURN NULL;
END;
$$;

CREATE TRIGGER model_catalog_changed AFTER INSERT OR UPDATE OR DELETE ON model_catalog FOR EACH ROW EXECUTE FUNCTION billing_catalog_changed();

CREATE TRIGGER model_prices_changed AFTER INSERT OR UPDATE OR DELETE ON model_prices FOR EACH ROW EXECUTE FUNCTION billing_catalog_changed();

CREATE TRIGGER model_image_prices_changed AFTER INSERT OR UPDATE OR DELETE ON model_image_prices FOR EACH ROW EXECUTE FUNCTION billing_catalog_changed();

CREATE TRIGGER operation_prices_changed AFTER INSERT OR UPDATE OR DELETE ON operation_prices FOR EACH ROW EXECUTE FUNCTION billing_catalog_changed();
