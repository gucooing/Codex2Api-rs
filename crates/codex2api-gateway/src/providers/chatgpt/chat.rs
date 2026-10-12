use crate::{GatewayError, Result};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn required<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| GatewayError::bad_request(format!("{key} must be a nonempty string")))
}

pub(crate) fn chat_to_responses(body: &Value) -> Result<Value> {
    const SUPPORTED: &[&str] = &[
        "model",
        "messages",
        "stream",
        "stream_options",
        "n",
        "tools",
        "tool_choice",
        "parallel_tool_calls",
        "reasoning_effort",
        "response_format",
        "service_tier",
        "prompt_cache_key",
        "store",
    ];
    for (key, value) in body
        .as_object()
        .ok_or_else(|| GatewayError::bad_request("Request body must be an object"))?
    {
        if !SUPPORTED.contains(&key.as_str()) && !value.is_null() {
            return Err(GatewayError::bad_request(format!(
                "Chat Completions parameter '{key}' is not supported by this platform"
            )));
        }
    }
    if body
        .get("n")
        .is_some_and(|value| !value.is_null() && value.as_u64() != Some(1))
    {
        return Err(GatewayError::bad_request("Only n=1 is supported"));
    }
    if body
        .get("store")
        .is_some_and(|value| !value.is_null() && value != false)
    {
        return Err(GatewayError::bad_request("Only store=false is supported"));
    }
    let messages = body["messages"]
        .as_array()
        .filter(|items| !items.is_empty())
        .ok_or_else(|| GatewayError::bad_request("messages must be a nonempty array"))?;
    let mut input = vec![];
    for message in messages {
        let role = required(message, "role")?;
        if message.as_object().is_none_or(|object| {
            object.iter().any(|(key, value)| {
                !matches!(
                    key.as_str(),
                    "role" | "content" | "tool_calls" | "tool_call_id" | "name"
                ) && !value.is_null()
            })
        }) {
            return Err(GatewayError::bad_request("Unsupported message field"));
        }
        if message.get("name").is_some_and(|value| !value.is_null()) {
            return Err(GatewayError::bad_request(
                "Named messages are not supported by this platform",
            ));
        }
        if role == "tool" {
            let call_id = required(message, "tool_call_id")?;
            let output = message["content"].as_str().ok_or_else(|| {
                GatewayError::bad_request("Tool message content must be a string")
            })?;
            input.push(json!({"type":"function_call_output","call_id":call_id,"output":output}));
            continue;
        }
        if !matches!(role, "system" | "developer" | "user" | "assistant") {
            return Err(GatewayError::bad_request("Unsupported message role"));
        }
        let content_type = if role == "assistant" {
            "output_text"
        } else {
            "input_text"
        };
        let content = match &message["content"] {
            Value::String(text) => vec![json!({"type":content_type,"text":text})],
            Value::Null if role == "assistant" && message["tool_calls"].is_array() => vec![],
            Value::Array(parts) => {
                let mut content = vec![];
                for part in parts {
                    match part["type"].as_str() {
                        Some("text") => content.push(json!({"type":content_type,"text":part["text"].as_str().ok_or_else(||GatewayError::bad_request("Text content must be a string"))?})),
                        Some("image_url") if role == "user" => {
                            let mut image = json!({"type":"input_image","image_url":required(&part["image_url"],"url")?});
                            if let Some(detail) = part["image_url"].get("detail") { image["detail"] = detail.clone(); }
                            content.push(image);
                        }
                        _ => return Err(GatewayError::bad_request("Unsupported message content type")),
                    }
                }
                content
            }
            _ => {
                return Err(GatewayError::bad_request(
                    "Message content must be text or supported content parts",
                ));
            }
        };
        if !content.is_empty() {
            input.push(json!({"type":"message","role":role,"content":content}));
        }
        if let Some(calls) = message.get("tool_calls").filter(|v| !v.is_null()) {
            if role != "assistant" {
                return Err(GatewayError::bad_request(
                    "tool_calls are valid only for assistant messages",
                ));
            }
            for call in calls
                .as_array()
                .ok_or_else(|| GatewayError::bad_request("tool_calls must be an array"))?
            {
                if call["type"] != "function" {
                    return Err(GatewayError::bad_request(
                        "Only function tools are supported",
                    ));
                }
                input.push(json!({"type":"function_call","call_id":required(call,"id")?,"name":required(&call["function"],"name")?,"arguments":call["function"]["arguments"].as_str().ok_or_else(||GatewayError::bad_request("Tool arguments must be a JSON string"))?}));
            }
        }
    }
    let mut output = json!({"model":required(body,"model")?,"input":input});
    for key in ["service_tier", "prompt_cache_key", "parallel_tool_calls"] {
        if let Some(value) = body.get(key).filter(|v| !v.is_null()) {
            output[key] = value.clone();
        }
    }
    if let Some(effort) = body.get("reasoning_effort").filter(|v| !v.is_null()) {
        if !effort.is_string() {
            return Err(GatewayError::bad_request(
                "reasoning_effort must be a string",
            ));
        }
        output["reasoning"] = json!({"effort":effort});
    }
    if let Some(tools) = body.get("tools").filter(|v| !v.is_null()) {
        let mut converted = vec![];
        for tool in tools
            .as_array()
            .ok_or_else(|| GatewayError::bad_request("tools must be an array"))?
        {
            if tool["type"] != "function" {
                return Err(GatewayError::bad_request(
                    "Only function tools are supported",
                ));
            }
            let mut function = tool["function"]
                .as_object()
                .cloned()
                .ok_or_else(|| GatewayError::bad_request("function must be an object"))?;
            required(&tool["function"], "name")?;
            function.insert("type".into(), "function".into());
            converted.push(Value::Object(function));
        }
        output["tools"] = converted.into();
    }
    if let Some(choice) = body.get("tool_choice").filter(|v| !v.is_null()) {
        output["tool_choice"] = match choice {
            Value::String(value) if matches!(value.as_str(), "auto" | "none" | "required") => {
                choice.clone()
            }
            Value::Object(_) if choice["type"] == "function" => {
                json!({"type":"function","name":required(&choice["function"],"name")?})
            }
            _ => return Err(GatewayError::bad_request("Unsupported tool_choice")),
        };
    }
    if let Some(format) = body.get("response_format").filter(|v| !v.is_null()) {
        output["text"] = match format["type"].as_str() {
            Some("text" | "json_object") => json!({"format":{"type":format["type"]}}),
            Some("json_schema") => {
                let mut schema = format["json_schema"]
                    .as_object()
                    .cloned()
                    .ok_or_else(|| GatewayError::bad_request("json_schema must be an object"))?;
                schema.insert("type".into(), "json_schema".into());
                json!({"format":schema})
            }
            _ => return Err(GatewayError::bad_request("Unsupported response_format")),
        };
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tool_call_history_and_image_inputs_keep_openai_semantics() {
        let body = json!({"model":"custom","messages":[{"role":"system","content":"Be precise"},{"role":"user","content":[{"type":"text","text":"Read this"},{"type":"image_url","image_url":{"url":"https://example.test/image.png","detail":"high"}}]},{"role":"assistant","content":null,"tool_calls":[{"id":"call_1","type":"function","function":{"name":"lookup","arguments":"{\"q\":1}"}}]},{"role":"tool","tool_call_id":"call_1","content":"found"}],"tools":[{"type":"function","function":{"name":"lookup","parameters":{"type":"object"},"strict":true}}],"tool_choice":{"type":"function","function":{"name":"lookup"}},"reasoning_effort":"high"});
        let value = chat_to_responses(&body).unwrap();
        assert_eq!(
            value["input"][1]["content"][1],
            json!({"type":"input_image","image_url":"https://example.test/image.png","detail":"high"})
        );
        assert_eq!(value["input"][2]["call_id"], "call_1");
        assert_eq!(
            value["input"][3],
            json!({"type":"function_call_output","call_id":"call_1","output":"found"})
        );
        assert_eq!(value["tools"][0]["type"], "function");
        assert_eq!(
            value["tool_choice"],
            json!({"type":"function","name":"lookup"})
        );
        assert_eq!(value["reasoning"]["effort"], "high");
    }
    #[test]
    fn unsupported_parameters_do_not_disappear() {
        for (field, value) in [
            ("temperature", json!(0.3)),
            ("n", json!(2)),
            ("stop", json!(["STOP"])),
            ("max_tokens", json!(100)),
        ] {
            let mut body = json!({"model":"custom","messages":[{"role":"user","content":"Hello"}]});
            body[field] = value;
            assert!(chat_to_responses(&body).is_err(), "{field}");
        }
    }
    #[test]
    fn streamed_tools_keep_indices_and_finish_after_real_usage() {
        let mut stream = ChatStream::new(true);
        let first=stream.event(&json!({"type":"response.created","response":{"id":"resp_1","model":"custom","created_at":123}})).unwrap();
        assert_eq!(first[0]["choices"][0]["delta"]["role"], "assistant");
        let tool=stream.event(&json!({"type":"response.output_item.added","output_index":3,"item":{"type":"function_call","call_id":"call_1","name":"lookup","arguments":""}})).unwrap();
        assert_eq!(tool[0]["choices"][0]["delta"]["tool_calls"][0]["index"], 0);
        let delta=stream.event(&json!({"type":"response.function_call_arguments.delta","output_index":3,"delta":"{\"q\":"})).unwrap();
        assert!(
            delta[0]["choices"][0]["delta"]["tool_calls"][0]
                .get("id")
                .is_none()
        );
        let done=stream.event(&json!({"type":"response.function_call_arguments.done","output_index":3,"arguments":"{\"q\":1}"})).unwrap();
        assert_eq!(
            done[0]["choices"][0]["delta"]["tool_calls"][0]["function"]["arguments"],
            "1}"
        );
        let complete=stream.event(&json!({"type":"response.completed","response":{"status":"completed","usage":{"input_tokens":100,"output_tokens":20,"input_tokens_details":{"cached_tokens":50},"output_tokens_details":{"reasoning_tokens":7}}}})).unwrap();
        assert_eq!(complete[0]["choices"][0]["finish_reason"], "tool_calls");
        assert!(complete[0]["usage"].is_null());
        assert_eq!(complete[1]["choices"], json!([]));
        assert_eq!(complete[1]["usage"]["completion_tokens"], 20);
        assert_eq!(
            complete[1]["usage"]["completion_tokens_details"]["reasoning_tokens"],
            7
        );
        assert!(
            stream
                .event(&json!({"type":"response.completed"}))
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn length_and_failure_do_not_turn_into_successful_stop() {
        let incomplete = json!({"status":"incomplete","incomplete_details":{"reason":"max_output_tokens"},"output":[]});
        assert_eq!(
            chat_response(&incomplete).unwrap()["choices"][0]["finish_reason"],
            "length"
        );
        let failure =
            json!({"type":"response.failed","response":{"error":{"code":"rate_limit_exceeded"}}});
        assert!(ChatStream::new(false).event(&failure).is_err());
        let completed = json!({"status":"completed","id":"resp_1","created_at":123,"model":"test","output":[{"type":"message","content":[{"type":"output_text","text":"hello"}]}],"usage":{"input_tokens":10,"output_tokens":4}});
        let json = chat_response(&completed).unwrap();
        assert_eq!(json["choices"][0]["message"]["content"], "hello");
        assert_eq!(json["usage"]["total_tokens"], 14);
    }
}

pub(crate) fn chat_usage(usage: &Value) -> Value {
    if !usage.is_object() {
        return Value::Null;
    }
    let mut result = json!({"prompt_tokens":usage["input_tokens"],"completion_tokens":usage["output_tokens"],"total_tokens":usage["total_tokens"]});
    if result["total_tokens"].is_null() {
        if let (Some(input), Some(output)) = (
            usage["input_tokens"].as_i64(),
            usage["output_tokens"].as_i64(),
        ) {
            result["total_tokens"] = input.saturating_add(output).into();
        }
    }
    if let Some(value) = usage.get("input_tokens_details") {
        result["prompt_tokens_details"] = value.clone();
    }
    if let Some(value) = usage.get("output_tokens_details") {
        result["completion_tokens_details"] = value.clone();
    }
    result
}

fn finish_reason(response: &Value, tools: bool) -> Result<&'static str> {
    match response["status"].as_str() {
        Some("completed") => Ok(if tools { "tool_calls" } else { "stop" }),
        Some("incomplete") => match response["incomplete_details"]["reason"].as_str() {
            Some("max_output_tokens") => Ok("length"),
            Some("content_filter") => Ok("content_filter"),
            _ => Err(GatewayError::upstream_incomplete()),
        },
        _ => Err(GatewayError::upstream_value(response)),
    }
}

pub(crate) fn chat_response(response: &Value) -> Result<Value> {
    let mut text = String::new();
    let mut refusal = String::new();
    let mut tools = vec![];
    for item in response["output"].as_array().into_iter().flatten() {
        if item["type"] == "function_call" {
            tools.push(json!({"id":item["call_id"],"type":"function","function":{"name":item["name"],"arguments":item["arguments"]}}));
        }
        for part in item["content"].as_array().into_iter().flatten() {
            if part["type"] == "output_text" {
                text.push_str(part["text"].as_str().unwrap_or(""));
            }
            if part["type"] == "refusal" {
                refusal.push_str(part["refusal"].as_str().unwrap_or(""));
            }
        }
    }
    let finish = finish_reason(response, !tools.is_empty())?;
    let mut message = json!({"role":"assistant","content":if text.is_empty() {Value::Null}else {text.into()},"refusal":if refusal.is_empty() {Value::Null}else {refusal.into()}});
    if !tools.is_empty() {
        message["tool_calls"] = tools.into();
    }
    Ok(
        json!({"id":response["id"],"object":"chat.completion","created":response["created_at"],"model":response["model"],"choices":[{"index":0,"message":message,"finish_reason":finish,"logprobs":null}],"usage":chat_usage(&response["usage"])}),
    )
}

#[derive(Default)]
pub(crate) struct ChatStream {
    id: Value,
    model: Value,
    created: Value,
    role_sent: bool,
    tools: BTreeMap<u64, (usize, String)>,
    finished: bool,
    pub include_usage: bool,
}
impl ChatStream {
    pub(crate) fn new(include_usage: bool) -> Self {
        Self {
            include_usage,
            ..Default::default()
        }
    }
    fn chunk(&self, delta: Value, finish: Value) -> Value {
        let mut value = json!({"id":self.id,"object":"chat.completion.chunk","created":self.created,"model":self.model,"choices":[{"index":0,"delta":delta,"finish_reason":finish,"logprobs":null}]});
        if self.include_usage {
            value["usage"] = Value::Null;
        }
        value
    }
    pub(crate) fn event(&mut self, event: &Value) -> Result<Vec<Value>> {
        if self.finished {
            return Ok(vec![]);
        }
        let response = &event["response"];
        for (target, key) in [
            (&mut self.id, "id"),
            (&mut self.model, "model"),
            (&mut self.created, "created_at"),
        ] {
            if let Some(value) = response.get(key) {
                *target = value.clone();
            }
        }
        let mut chunks = vec![];
        if !self.role_sent && !self.id.is_null() {
            self.role_sent = true;
            chunks.push(self.chunk(json!({"role":"assistant","content":""}), Value::Null));
        }
        match event["type"].as_str() {
            Some("response.output_text.delta") => {
                chunks.push(self.chunk(json!({"content":event["delta"]}), Value::Null))
            }
            Some("response.refusal.delta") => {
                chunks.push(self.chunk(json!({"refusal":event["delta"]}), Value::Null))
            }
            Some("response.output_item.added") if event["item"]["type"] == "function_call" => {
                let output_index = event["output_index"]
                    .as_u64()
                    .ok_or_else(GatewayError::upstream_incomplete)?;
                let index = self.tools.len();
                let item = &event["item"];
                let arguments = item["arguments"].as_str().unwrap_or("").to_owned();
                if !self.tools.contains_key(&output_index) {
                    self.tools.insert(output_index, (index, arguments.clone()));
                    chunks.push(self.chunk(json!({"tool_calls":[{"index":index,"id":item["call_id"],"type":"function","function":{"name":item["name"],"arguments":arguments}}]}), Value::Null));
                }
            }
            Some("response.function_call_arguments.delta") => {
                let output_index = event["output_index"]
                    .as_u64()
                    .ok_or_else(GatewayError::upstream_incomplete)?;
                let delta = event["delta"]
                    .as_str()
                    .ok_or_else(GatewayError::upstream_incomplete)?;
                let (index, arguments) = self
                    .tools
                    .get_mut(&output_index)
                    .ok_or_else(GatewayError::upstream_incomplete)?;
                arguments.push_str(delta);
                let index = *index;
                chunks.push(self.chunk(
                    json!({"tool_calls":[{"index":index,"function":{"arguments":delta}}]}),
                    Value::Null,
                ));
            }
            Some("response.function_call_arguments.done") => {
                let output_index = event["output_index"]
                    .as_u64()
                    .ok_or_else(GatewayError::upstream_incomplete)?;
                let full = event["arguments"]
                    .as_str()
                    .ok_or_else(GatewayError::upstream_incomplete)?;
                let (index, arguments) = self
                    .tools
                    .get_mut(&output_index)
                    .ok_or_else(GatewayError::upstream_incomplete)?;
                let suffix = full
                    .strip_prefix(arguments.as_str())
                    .ok_or_else(GatewayError::upstream_incomplete)?
                    .to_owned();
                arguments.push_str(&suffix);
                let index = *index;
                if !suffix.is_empty() {
                    chunks.push(self.chunk(
                        json!({"tool_calls":[{"index":index,"function":{"arguments":suffix}}]}),
                        Value::Null,
                    ));
                }
            }
            Some("response.completed" | "response.done" | "response.incomplete") => {
                let reason = finish_reason(response, !self.tools.is_empty())?;
                chunks.push(self.chunk(json!({}), reason.into()));
                if self.include_usage {
                    let mut usage = self.chunk(json!({}), Value::Null);
                    usage["choices"] = json!([]);
                    usage["usage"] = chat_usage(&response["usage"]);
                    chunks.push(usage);
                }
                self.finished = true;
            }
            Some("response.failed" | "error") => return Err(GatewayError::upstream_value(event)),
            _ => {}
        }
        Ok(chunks)
    }
}
