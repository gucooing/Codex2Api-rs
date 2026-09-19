use super::esc;
use codex2api_storage::{Account, TurnStateCache, TurnStateSettings};

pub(crate) fn render(
    account: &Account,
    csrf: &str,
    settings: &TurnStateSettings,
    entries: &[TurnStateCache],
    now: i64,
) -> String {
    let mut cards = String::new();
    for entry in entries {
        let state = if !settings.enabled {
            "已关闭"
        } else if entry.lease_until > now {
            "正在采集"
        } else if entry.token.is_some() && entry.expires_at > now {
            "缓存可用"
        } else if entry.next_probe_at > now {
            "冷却中"
        } else {
            "等待采集"
        };
        let probe = match entry.probe_result.as_str() {
            "accepted" => "采集成功",
            "timeout" => "探针超时",
            "http_error" => "上游请求失败",
            "network_error" => "连接失败",
            "invalid_state" => "状态格式不匹配",
            "incomplete_stream" => "探针未完整完成",
            "invalid_probe" => "探针配置无效",
            _ => "尚未探测",
        };
        let time = chrono::DateTime::from_timestamp(entry.last_probe_at, 0)
            .filter(|_| entry.last_probe_at != 0)
            .map(|d| d.to_rfc3339())
            .unwrap_or_else(|| "—".into());
        let shape = entry
            .token
            .as_ref()
            .map(|t| format!("{} 字符 / 10 块", t.len()))
            .unwrap_or_else(|| "—".into());
        cards.push_str(&format!(r#"<section class="card"><h3>{model}</h3><dl class="dl">
<dt>缓存状态</dt><dd>{state}</dd><dt>状态长度</dt><dd>{shape}</dd>
<dt>策略有效期剩余</dt><dd>{ttl} 秒</dd><dt>距离续期</dt><dd>{renew} 秒（匹配请求触发）</dd>
<dt>探针冷却剩余</dt><dd>{cooldown} 秒</dd><dt>最近探针</dt><dd>{probe} / HTTP {status}</dd>
<dt>探针时间（UTC）</dt><dd>{time}</dd><dt>累计注入次数</dt><dd>{injections}</dd>
<dt>客户端 state 优先次数</dt><dd>{clients}</dd><dt>连续不合格响应</dt><dd>{strikes}</dd></dl></section>"#,
            model=esc(&entry.model), shape=esc(&shape), ttl=(entry.expires_at-now).max(0), renew=(entry.refresh_at-now).max(0),
            cooldown=(entry.next_probe_at-now).max(0), status=if entry.probe_status > 0 { entry.probe_status.to_string() } else { "—".into() }, time=esc(&time), injections=entry.injections,
            clients=entry.client_states, strikes=entry.strikes));
    }
    if cards.is_empty() {
        cards.push_str("<section class=\"card\"><p>尚无缓存记录。启用后，匹配模型且未携带 state 的 HTTP Responses 请求会触发采集。</p></section>");
    }
    format!(
        r#"<section class="card"><h2>实验性 turn-state 复用</h2>
<p class="muted">在同一账户、同一模型的请求间复用路由状态。默认关闭；跨轮次复用偏离官方协议，不保证改善回答质量。</p>
<p>适用范围：HTTP Responses。WebSocket 不参与实验。已有客户端 state 优先；缓存可用不等于每次请求都会注入。</p>
<form class="stack" method="post" action="/admin/accounts/{id}/turn-state">
<input type="hidden" name="csrf" value="{csrf}">
<label class="row-actions"><input type="checkbox" name="enabled" {checked}> 启用本账户的状态复用</label>
<label>适用模型（每行一个）<textarea name="models" rows="3" required>{models}</textarea></label>
<label>缓存 TTL（秒）<input type="number" name="ttl" min="120" max="3600" value="{ttl}" required></label>
<label>提前续期（秒，必须小于 TTL）<input type="number" name="renew" min="30" max="3599" value="{renew}" required></label>
<label>失败冷却（秒）<input type="number" name="cooldown" min="30" max="3600" value="{cooldown}" required></label>
<p class="muted">仅接受 10 块状态；时间配置是本地策略。探针最多等待 20 秒，会产生额外上游请求。保存配置会清除旧缓存及统计。</p>
<button type="submit">保存配置</button></form></section>
<section class="card"><div class="row-actions"><a class="btn secondary" href="/admin/accounts/{id}?tab=turn-state">刷新状态</a>
<form method="post" action="/admin/accounts/{id}/turn-state/clear"><input type="hidden" name="csrf" value="{csrf}"><button class="secondary" type="submit">清除本账户缓存</button></form></div>
<p class="muted">仅展示状态摘要，不展示完整令牌。探针不计入用户请求用量列表。</p></section>{cards}"#,
        id = esc(&account.id),
        csrf = esc(csrf),
        checked = if settings.enabled { "checked" } else { "" },
        models = esc(&settings.models.join("\n")),
        ttl = settings.ttl,
        renew = settings.renew,
        cooldown = settings.cooldown
    )
}
