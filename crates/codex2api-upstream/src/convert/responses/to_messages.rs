//! Messages callers served by a Responses-family wire: events to Messages events.
use crate::convert::messages_output::{Kind, Writer};
use crate::convert::usage::Tokens;
use crate::inbound::{Error, Result};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Responses events to Messages events, one open content block at a time.
#[derive(Default)]
pub struct Stream {
    writer: Writer,
    summary: BTreeMap<u64, u64>,
    finished: bool,
}

const THINKING: u64 = 0;
fn text_key(output: u64, part: u64) -> (u64, u64) {
    (output, part + 1)
}

impl Stream {
    fn thinking(&mut self, output: u64, out: &mut Vec<Value>) {
        self.writer.ensure(
            (output, THINKING),
            Kind::Thinking,
            json!({"type":"thinking","thinking":""}),
            out,
        );
    }
    fn text(
        &mut self,
        output: u64,
        part: u64,
        text: &str,
        done: bool,
        out: &mut Vec<Value>,
    ) -> Result<()> {
        let key = text_key(output, part);
        if text.is_empty() {
            return Ok(());
        }
        self.writer
            .ensure(key, Kind::Text, json!({"type":"text","text":""}), out);
        if done {
            self.writer.reconcile(key, text, out)
        } else {
            self.writer.append(key, text, out)
        }
    }
    fn item(&mut self, output: u64, item: &Value, out: &mut Vec<Value>) -> Result<()> {
        match item["type"].as_str() {
            Some("reasoning") => {
                self.thinking(output, out);
                let text = crate::convert::responses::output::reasoning_text(item);
                self.writer.reconcile((output, THINKING), &text, out)?;
                self.writer.signature(
                    (output, THINKING),
                    item["encrypted_content"].as_str().unwrap_or(""),
                );
                self.writer.close((output, THINKING), out);
            }
            Some("function_call") => {
                self.tool(output, item, out);
                self.writer.reconcile(
                    (output, 0),
                    item["arguments"].as_str().unwrap_or(""),
                    out,
                )?;
                self.writer.close((output, 0), out);
            }
            Some("message") => {
                for (part, value) in item["content"].as_array().into_iter().flatten().enumerate() {
                    let text = value["text"]
                        .as_str()
                        .or_else(|| value["refusal"].as_str())
                        .unwrap_or("");
                    self.text(output, part as u64, text, true, out)?;
                    self.writer.close(text_key(output, part as u64), out);
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn tool(&mut self, output: u64, item: &Value, out: &mut Vec<Value>) {
        self.writer.ensure(
            (output, 0),
            Kind::Tool,
            json!({"type":"tool_use","id":item["call_id"],"name":item["name"],"input":{}}),
            out,
        );
    }

    pub(crate) fn event(&mut self, event: &Value) -> Result<Vec<Value>> {
        let mut out = Vec::new();
        if self.finished {
            return Ok(out);
        }
        let response = &event["response"];
        if !self.writer.started() {
            out.extend(self.writer.start(&response["id"], &response["model"]));
        }
        let output = event["output_index"].as_u64().unwrap_or(0);
        let part = event["content_index"].as_u64().unwrap_or(0);
        let delta = event["delta"].as_str().unwrap_or("");
        match event["type"].as_str().unwrap_or("") {
            "response.output_item.added" => match event["item"]["type"].as_str() {
                Some("reasoning") => self.thinking(output, &mut out),
                Some("function_call") => self.tool(output, &event["item"], &mut out),
                _ => {}
            },
            "response.output_text.delta" | "response.refusal.delta" => {
                self.text(output, part, delta, false, &mut out)?
            }
            "response.output_text.done" => self.text(
                output,
                part,
                event["text"].as_str().unwrap_or(""),
                true,
                &mut out,
            )?,
            "response.reasoning_summary_text.delta" | "response.reasoning_text.delta" => {
                self.thinking(output, &mut out);
                let index = event["summary_index"].as_u64().unwrap_or(0);
                let last = self.summary.insert(output, index);
                if last.is_some_and(|last| last != index) {
                    self.writer.append((output, THINKING), "\n\n", &mut out)?;
                }
                self.writer.append((output, THINKING), delta, &mut out)?;
            }
            "response.function_call_arguments.delta" => {
                if !self.writer.has((output, 0)) {
                    return Err(Error::upstream_incomplete());
                }
                self.writer.append((output, 0), delta, &mut out)?;
            }
            "response.function_call_arguments.done" => self.writer.reconcile(
                (output, 0),
                event["arguments"].as_str().unwrap_or(""),
                &mut out,
            )?,
            "response.output_item.done" => self.item(output, &event["item"], &mut out)?,
            "response.completed" | "response.done" | "response.incomplete" => {
                for (index, item) in response["output"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .enumerate()
                {
                    self.item(index as u64, item, &mut out)?;
                }
                let stop = match response["status"].as_str() {
                    Some("completed") if self.writer.tool_use() => "tool_use",
                    Some("completed") => "end_turn",
                    Some("incomplete") => match response
                        .pointer("/incomplete_details/reason")
                        .and_then(Value::as_str)
                    {
                        Some("max_output_tokens") => "max_tokens",
                        Some("content_filter") => "refusal",
                        _ => return Err(Error::upstream_incomplete()),
                    },
                    _ => return Err(Error::upstream_value(response)),
                };
                let usage = Tokens::from_responses(&response["usage"]);
                if !usage.known() {
                    return Err(Error::upstream_incomplete());
                }
                out.extend(self.writer.finish(stop, usage.messages()));
                self.finished = true;
            }
            _ => {}
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn messages() -> Value {
        json!({"model":"custom-model","max_tokens":512,"system":[{"type":"text","text":"x-anthropic-billing-header: cc_version=1"},{"type":"text","text":"policy"}],"messages":[
            {"role":"user","content":[{"type":"text","text":"look"},{"type":"image","source":{"type":"base64","media_type":"image/png","data":"aGVsbG8="}}]},
            {"role":"assistant","content":[{"type":"thinking","thinking":"plan","signature":"gAAAA-cipher"},{"type":"text","text":"checking"},{"type":"tool_use","id":"call_1","name":"lookup","input":{"city":"Taipei"}}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"call_1","content":"sunny"},{"type":"text","text":"thanks"}]}
        ],"tools":[{"name":"lookup","input_schema":{"type":"object"}},{"type":"web_search_20250305","name":"web_search"}],
        "tool_choice":{"type":"tool","name":"lookup","disable_parallel_tool_use":true},"stop_sequences":["x"],"top_k":3})
    }

    #[test]
    fn request_keeps_order_images_reasoning_and_tool_round_trip() {
        let body = crate::convert::responses::request::from_messages(&messages()).unwrap();
        let input = body["input"].as_array().unwrap();
        assert_eq!(input[0]["role"], "system");
        assert_eq!(input[0]["content"].as_array().unwrap().len(), 1);
        assert_eq!(
            input[1]["content"][1]["image_url"],
            "data:image/png;base64,aGVsbG8="
        );
        assert_eq!(input[2]["type"], "reasoning");
        assert_eq!(input[2]["encrypted_content"], "gAAAA-cipher");
        assert_eq!(input[3]["content"][0]["type"], "output_text");
        assert_eq!(input[4]["call_id"], "call_1");
        assert_eq!(input[5]["output"], "sunny");
        assert_eq!(input[6]["content"][0]["text"], "thanks");
        assert_eq!(body["tools"][0]["parameters"]["properties"], json!({}));
        assert_eq!(body["tools"][1], json!({"type":"web_search"}));
        assert_eq!(
            body["tool_choice"],
            json!({"type":"function","name":"lookup"})
        );
        assert_eq!(body["parallel_tool_calls"], false);
        let mut parallel = messages();
        parallel["tool_choice"]["disable_parallel_tool_use"] = false.into();
        assert_eq!(
            crate::convert::responses::request::from_messages(&parallel).unwrap()["parallel_tool_calls"],
            true
        );
        parallel["tool_choice"]["disable_parallel_tool_use"] = "false".into();
        assert!(crate::convert::responses::request::from_messages(&parallel).is_err());
        assert_eq!(
            body["reasoning"],
            json!({"effort":"medium","summary":"auto"})
        );
        assert!(body.get("stop_sequences").is_none() && body.get("top_k").is_none());
    }

    #[test]
    fn stream_serializes_blocks_recovers_snapshots_and_reports_usage() {
        let mut stream = Stream::default();
        let mut events = Vec::new();
        for event in [
            json!({"type":"response.created","response":{"id":"resp_1","model":"custom"}}),
            json!({"type":"response.output_item.added","output_index":0,"item":{"type":"reasoning"}}),
            json!({"type":"response.reasoning_summary_text.delta","output_index":0,"summary_index":0,"delta":"think"}),
            json!({"type":"response.output_item.done","output_index":0,"item":{"type":"reasoning","summary":[{"type":"summary_text","text":"think"}],"encrypted_content":"cipher"}}),
            json!({"type":"response.output_item.added","output_index":1,"item":{"type":"function_call","call_id":"c0","name":"lookup"}}),
            json!({"type":"response.output_item.added","output_index":2,"item":{"type":"function_call","call_id":"c1","name":"lookup"}}),
            json!({"type":"response.function_call_arguments.delta","output_index":2,"delta":"{}"}),
            json!({"type":"response.function_call_arguments.delta","output_index":1,"delta":"{\"a\":"}),
            json!({"type":"response.completed","response":{"id":"resp_1","model":"custom","status":"completed","output":[
                {"type":"reasoning","summary":[{"type":"summary_text","text":"think"}],"encrypted_content":"cipher"},
                {"type":"function_call","call_id":"c0","name":"lookup","arguments":"{\"a\":1}"},
                {"type":"function_call","call_id":"c1","name":"lookup","arguments":"{}"},
                {"type":"message","content":[{"type":"output_text","text":"done"}]}],
                "usage":{"input_tokens":100,"input_tokens_details":{"cached_tokens":40,"cache_write_tokens":10},"output_tokens":20}}}),
        ] {
            events.extend(stream.event(&event).unwrap());
        }
        let mut open = None;
        for event in &events {
            match event["type"].as_str() {
                Some("content_block_start") => {
                    assert!(open.is_none());
                    open = event["index"].as_u64();
                }
                Some("content_block_delta") => assert_eq!(open, event["index"].as_u64()),
                Some("content_block_stop") => {
                    assert_eq!(open, event["index"].as_u64());
                    open = None;
                }
                _ => {}
            }
        }
        let signature = events
            .iter()
            .position(|e| e["delta"]["type"] == "signature_delta")
            .unwrap();
        assert_eq!(events[signature + 1]["type"], "content_block_stop");
        let arguments = events
            .iter()
            .filter(|e| e["index"] == 1)
            .filter_map(|e| e["delta"]["partial_json"].as_str())
            .collect::<String>();
        assert_eq!(arguments, "{\"a\":1}");
        let delta = &events[events.len() - 2];
        assert_eq!(delta["delta"]["stop_reason"], "tool_use");
        assert_eq!(delta["usage"]["input_tokens"], 50);
        assert_eq!(delta["usage"]["cache_read_input_tokens"], 40);
        assert_eq!(delta["usage"]["cache_creation_input_tokens"], 10);
        let mut output = crate::convert::messages_output::Output::default();
        for event in events {
            output.push(event).unwrap();
        }
        let message = output.finish().unwrap();
        assert_eq!(message["content"][0]["signature"], "cipher");
        assert_eq!(message["content"][1]["input"], json!({"a":1}));
        assert_eq!(message["content"][3]["text"], "done");
        assert_eq!(message["usage"]["output_tokens"], 20);
    }
}
