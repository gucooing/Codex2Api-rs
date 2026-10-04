//! Grok Build's JWT tier, /user enum and /settings display name are distinct.
//! Reference: xai-grok-shell/src/agent/mvp_agent/mod.rs and telemetry/client.rs.

pub const SUBSCRIPTION_TIERS: &[&str] = &[
    "free",
    "x_basic",
    "x_premium",
    "x_premium_plus",
    "supergrok_lite",
    "supergrok",
    "supergrok_plus",
    "supergrok_heavy",
    // Retain previously saved plans without rewriting subscription/order history.
    "premium",
    "premium_plus",
    "supergrok_pro",
    "team",
];

pub struct SubscriptionTier {
    pub id: &'static str,
    pub display: &'static str,
    pub user_tier: &'static str,
    pub jwt_tier: u8,
}

pub fn subscription(tier: &str) -> Option<SubscriptionTier> {
    let (id, display, user_tier, jwt_tier) = match tier {
        "free" | "Free" => ("free", "Free", "Free", 0),
        "supergrok" | "GrokPro" | "SuperGrok" => ("supergrok", "SuperGrok", "GrokPro", 1),
        "x_basic" | "XBasic" => ("x_basic", "X Basic", "XBasic", 2),
        "x_premium" | "premium" | "XPremium" => ("x_premium", "X Premium", "XPremium", 3),
        "x_premium_plus" | "premium_plus" | "XPremiumPlus" => {
            ("x_premium_plus", "X Premium+", "XPremiumPlus", 4)
        }
        "supergrok_heavy" | "supergrok_pro" | "SuperGrokPro" | "SuperGrokHeavy" => {
            ("supergrok_heavy", "SuperGrok Heavy", "SuperGrokPro", 5)
        }
        "supergrok_lite" | "SuperGrokLite" => {
            ("supergrok_lite", "SuperGrok Lite", "SuperGrokLite", 6)
        }
        "supergrok_plus" | "SuperGrokPlus" => {
            ("supergrok_plus", "SuperGrok Plus", "SuperGrokPlus", 7)
        }
        _ => return None,
    };
    Some(SubscriptionTier {
        id,
        display,
        user_tier,
        jwt_tier,
    })
}
