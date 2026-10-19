//! Reported Responses token counts in Messages form. Missing counts stay missing.
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Tokens {
    /// All prompt tokens, including cache reads and writes.
    pub input: Option<u64>,
    pub cached: Option<u64>,
    pub written: Option<u64>,
    pub output: Option<u64>,
}

impl Tokens {
    pub fn from_responses(usage: &Value) -> Self {
        Self {
            input: usage["input_tokens"].as_u64(),
            cached: usage
                .pointer("/input_tokens_details/cached_tokens")
                .and_then(Value::as_u64),
            written: usage
                .pointer("/input_tokens_details/cache_write_tokens")
                .and_then(Value::as_u64),
            output: usage["output_tokens"].as_u64(),
        }
    }
    pub fn known(&self) -> bool {
        self.input.is_some() && self.output.is_some()
    }
    /// Messages reports uncached input separately from cache reads and writes.
    pub fn messages(&self) -> Value {
        let uncached = self.input.and_then(|input| {
            input
                .checked_sub(self.cached.unwrap_or(0))?
                .checked_sub(self.written.unwrap_or(0))
        });
        json!({"input_tokens":uncached,"cache_creation_input_tokens":self.written.unwrap_or(0),
            "cache_read_input_tokens":self.cached.unwrap_or(0),"output_tokens":self.output})
    }
}
