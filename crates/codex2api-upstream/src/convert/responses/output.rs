//! The synchronous Responses collector and reasoning text helper.
use crate::inbound::{Error, Result};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// Holds complete output items to assemble a synchronous response, or to
/// complete a terminal envelope that omits its already-streamed output.
#[derive(Default)]
pub struct Output {
    response: Map<String, Value>,
    items: BTreeMap<u64, (Value, usize)>,
    bytes: usize,
    terminal: Option<Value>,
}
impl Output {
    pub fn observe(&mut self, value: &mut Value) -> Result<()> {
        if value["type"] == "response.output_item.done"
            && let (Some(index), Some(item)) = (value["output_index"].as_u64(), value.get("item"))
        {
            let bytes = serde_json::to_vec(item)
                .map_err(|_| Error::upstream_incomplete())?
                .len();
            self.bytes = self
                .bytes
                .saturating_sub(self.items.get(&index).map_or(0, |(_, n)| *n))
                .saturating_add(bytes);
            if self.bytes > crate::MAX_REQUEST_BYTES {
                return Err(Error::upstream_incomplete());
            }
            self.items.insert(index, (item.clone(), bytes));
        }
        let terminal = crate::ResponseOutcome::from_value(value, None).is_some();
        if let Some(response) = value.get_mut("response").and_then(Value::as_object_mut) {
            if terminal {
                for (key, item) in std::mem::take(&mut self.response) {
                    response.entry(key).or_insert(item);
                }
                if !self.items.is_empty() {
                    let mut items: BTreeMap<u64, Value> = response
                        .remove("output")
                        .and_then(|value| value.as_array().cloned())
                        .unwrap_or_default()
                        .into_iter()
                        .enumerate()
                        .map(|(index, value)| (index as u64, value))
                        .collect();
                    items.extend(
                        std::mem::take(&mut self.items)
                            .into_iter()
                            .map(|(index, (value, _))| (index, value)),
                    );
                    response.insert("output".into(), Value::Array(items.into_values().collect()));
                }
            } else {
                self.response.extend(
                    response
                        .iter()
                        .filter(|(key, _)| !matches!(key.as_str(), "output" | "status"))
                        .map(|(key, value)| (key.clone(), value.clone())),
                );
            }
        }
        Ok(())
    }
    pub(crate) fn push(&mut self, mut event: Value) -> Result<()> {
        self.observe(&mut event)?;
        if crate::ResponseOutcome::from_value(&event, None).is_some() {
            self.terminal = Some(event["response"].take());
        }
        Ok(())
    }
    pub(crate) fn finish(self) -> Result<Value> {
        self.terminal.ok_or_else(Error::upstream_incomplete)
    }
}

/// Text carried by a reasoning item's summary and content parts.
pub(crate) fn reasoning_text(item: &Value) -> String {
    ["summary", "content"]
        .iter()
        .flat_map(|key| item[*key].as_array().into_iter().flatten())
        .filter_map(|part| part["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n\n")
}
