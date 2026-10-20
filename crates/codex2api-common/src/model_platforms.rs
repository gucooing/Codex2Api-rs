//! Model platforms: the vendor label of every catalog model. Platforms are independent
//! of the supplier channels that serve their models: most vendors have no subscription
//! login, and one channel (Antigravity, an API key) serves models of several vendors.
//! A label identifies a model only; it never restricts routing.

pub const OPENAI: &str = "openai";
pub const ANTHROPIC: &str = "anthropic";
pub const GOOGLE: &str = "google";
pub const XAI: &str = "xai";
pub const DEEPSEEK: &str = "deepseek";
pub const ALIBABA: &str = "alibaba";
pub const ZHIPU: &str = "zhipu";
pub const MOONSHOT: &str = "moonshot";
pub const MINIMAX: &str = "minimax";
pub const BYTEDANCE: &str = "bytedance";
pub const XIAOMI: &str = "xiaomi";
pub const TENCENT: &str = "tencent";
pub const BAIDU: &str = "baidu";
pub const STEPFUN: &str = "stepfun";
pub const META: &str = "meta";
pub const MISTRAL: &str = "mistral";
/// Models of a vendor that has no label of its own yet; administrators relabel them
/// once the vendor is listed.
pub const OTHER: &str = "other";

/// Every platform label a catalog model may carry.
pub const MODEL_PLATFORMS: &[&str] = &[
    OPENAI, ANTHROPIC, GOOGLE, XAI, DEEPSEEK, ALIBABA, ZHIPU, MOONSHOT, MINIMAX, BYTEDANCE, XIAOMI,
    TENCENT, BAIDU, STEPFUN, META, MISTRAL, OTHER,
];

pub fn valid_model_platform(label: &str) -> bool {
    MODEL_PLATFORMS.contains(&label)
}
