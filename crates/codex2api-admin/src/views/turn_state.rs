use super::esc;
use codex2api_storage::{Account, TurnStateCache, TurnStateSettings};

fn result_label(result: &str) -> &'static str {
    match result {
        "accepted" => "已采集合格 state",
        "older_state" => "合格，但保留了时间更新的缓存",
        "missing_header" => "未携带 state 头",
        "wrong_blocks" => "块数不符合条件（要求 10 块）",
        "invalid_encoding" => "state 编码不合法",
        "invalid_envelope" => "state 结构不符合条件",
        "invalid_timestamp" => "state 时间戳不合法",
        "future_state" => "state 时间戳超前，请检查系统时间",
        "expired" => "state 超出本地有效期",
        "http_error" => "上游 HTTP 请求失败，未采集",
        "network_error" => "上游连接失败",
        "cache_preferred" => "已使用目标缓存（覆盖客户端 state）",
        _ => "尚未观察到",
    }
}
fn time(value: i64) -> String {
    chrono::DateTime::from_timestamp(value, 0)
        .filter(|_| value > 0)
        .map(|date| date.to_rfc3339())
        .unwrap_or_else(|| "—".into())
}
fn http_status(value: i64) -> String {
    if value > 0 {
        value.to_string()
    } else {
        "—".into()
    }
}

pub(crate) fn render(
    account: &Account,
    csrf: &str,
    settings: &TurnStateSettings,
    entries: &[TurnStateCache],
    now: i64,
) -> String {
    let mut cards = String::new();
    for model in &settings.models {
        let Some(e) = entries.iter().find(|e| &e.model == model) else {
            cards.push_str(&format!(
                r#"<section class="card"><h3>{}</h3><p>{}</p></section>"#,
                esc(model),
                if settings.enabled {
                    "尚未观察到此账户、此模型的 HTTP Responses 请求。"
                } else {
                    "状态复用已关闭。"
                }
            ));
            continue;
        };
        let state = if !settings.enabled {
            "已关闭"
        } else if e.token.is_some() && e.expires_at > now {
            "缓存可用"
        } else if e.token.is_some() {
            "缓存已过期"
        } else {
            "尚无合格缓存，请查看下方采集结果"
        };
        let source = match e.source.as_str() {
            "client" => "客户端请求",
            "response" => "正常上游响应",
            _ => "—",
        };
        let shape = e
            .token
            .as_ref()
            .map(|t| format!("{} 字符 / 10 块", t.len()))
            .unwrap_or_else(|| "—".into());
        cards.push_str(&format!(r#"<section class="card"><h3>{model}</h3><dl class="dl">
<dt>缓存状态</dt><dd>{state}</dd><dt>缓存来源</dt><dd>{source}</dd>
<dt>采集时间（UTC）</dt><dd>{captured}</dd><dt>状态长度</dt><dd>{shape}</dd>
<dt>策略有效期剩余</dt><dd>{ttl} 秒</dd>
<dt>正常请求 / 响应次数</dt><dd>{requests} / {responses}</dd>
<dt>客户端 / 响应采集次数</dt><dd>{client_captures} / {response_captures}</dd>
<dt>最近客户端 state</dt><dd>{request_result}；{request_length} 字符 / {request_blocks} 块</dd>
<dt>客户端观察时间（UTC）</dt><dd>{request_time}</dd>
<dt>最近正常响应 state</dt><dd>{response_result}；HTTP {response_status}；{response_length} 字符 / {response_blocks} 块</dd>
<dt>响应观察时间（UTC）</dt><dd>{response_time}</dd>
<dt>累计缓存使用次数</dt><dd>{injections}</dd>
</dl></section>"#,
            model=esc(model), captured=time(e.captured_at), shape=esc(&shape),
            ttl=(e.expires_at-now).max(0),
            requests=e.request_count, responses=e.response_count,
            client_captures=e.request_captures, response_captures=e.response_captures,
            request_result=result_label(&e.request_result), request_length=e.request_length, request_blocks=e.request_blocks,
            request_time=time(e.last_request_at), response_result=result_label(&e.response_result),
            response_status=http_status(e.response_status), response_length=e.response_length, response_blocks=e.response_blocks,
            response_time=time(e.last_response_at), injections=e.injections,
        ));
    }
    format!(
        r#"<section class="card"><h2>实验性 turn-state 复用</h2>
<p>仅从正常流量采集：无有效缓存时采集客户端携带的目标 state，也会采集正常上游响应头中的目标 state。一旦缓存可用，后续请求统一使用缓存，覆盖客户端携带的 state。</p>
<p class="muted">按账户和模型隔离，仅适用 HTTP Responses。WebSocket 不参与。10 块结构和有效期是本地筛选条件，不代表已验证令牌签名，也不保证回答质量。</p>
<form class="stack" method="post" action="/admin/accounts/{id}/turn-state">
<input type="hidden" name="csrf" value="{csrf}">
<label class="row-actions"><input type="checkbox" name="enabled" {checked}> 启用本账户的状态复用</label>
<label>适用模型（每行一个，精确匹配）<textarea name="models" rows="3" required>{models}</textarea></label>
<label>缓存 TTL（秒）<input type="number" name="ttl" min="120" max="3600" value="{ttl}" required></label>
<p class="muted">有效期按 state 自带时间戳计算，并预留 30 秒。保存配置会清除旧缓存和统计。</p>
<button type="submit">保存配置</button></form></section>
<section class="card"><div class="row-actions"><a class="btn secondary" href="/admin/accounts/{id}?tab=turn-state">刷新状态</a>
<form method="post" action="/admin/accounts/{id}/turn-state/clear"><input type="hidden" name="csrf" value="{csrf}"><button class="secondary" type="submit">清除本账户缓存</button></form></div>
<p class="muted">仅显示摘要，不显示完整 state。正常响应成功不一定携带合格的 state；查看具体观察结果。不发送额外采集请求。</p></section>{cards}"#,
        id = esc(&account.id),
        csrf = esc(csrf),
        checked = if settings.enabled { "checked" } else { "" },
        models = esc(&settings.models.join("\n")),
        ttl = settings.ttl
    )
}
