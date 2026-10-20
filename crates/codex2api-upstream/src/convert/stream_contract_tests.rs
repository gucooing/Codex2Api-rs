//! Stateful client regressions: item identity, streamed content and terminal
//! snapshots must agree, including interleaved tool calls.
use super::*;
use serde_json::json;
use std::collections::{BTreeMap, HashMap, HashSet};

fn fixture(wire: Wire) -> Vec<Value> {
    match wire {
        Wire::Gemini => vec![
            json!({"response":{"candidates":[{"content":{"parts":[{"thought":true,"text":"why","thoughtSignature":"signed"}]}}]}}),
            json!({"response":{"candidates":[{"content":{"parts":[{"text":"Hel"}]}}]}}),
            json!({"response":{"candidates":[{"content":{"parts":[{"text":"lo"}]},"groundingMetadata":{"groundingChunks":[{"web":{"uri":"https://example.com","title":"Source"}}]}}]}}),
            json!({"response":{"candidates":[{"content":{"parts":[
                {"functionCall":{"id":"call_one","name":"city","args":{"city":"Taipei"}}},
                {"functionCall":{"id":"call_two","name":"unit","args":{"unit":"C"}}}
            ]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":10,"candidatesTokenCount":5}}}),
        ],
        Wire::ChatCompletions => vec![
            json!({"id":"chat_1","model":"model","choices":[{"delta":{"reasoning_content":"why"}}]}),
            json!({"choices":[{"delta":{"content":"Hel"}}]}),
            json!({"choices":[{"delta":{"content":"lo"}}]}),
            json!({"choices":[{"delta":{"tool_calls":[
                {"index":0,"id":"call_one","type":"function","function":{"name":"city","arguments":"{\"city\":"}},
                {"index":1,"id":"call_two","type":"function","function":{"name":"unit","arguments":"{\"unit\":"}}
            ]}}]}),
            json!({"choices":[{"delta":{"tool_calls":[
                {"index":1,"function":{"arguments":"\"C\"}"}},
                {"index":0,"function":{"arguments":"\"Taipei\"}"}}
            ]},"finish_reason":"tool_calls"}]}),
            json!({"choices":[],"usage":{"prompt_tokens":10,"completion_tokens":5}}),
        ],
        Wire::Messages => vec![
            json!({"type":"message_start","message":{"id":"msg_1","model":"model","usage":{"input_tokens":10,"output_tokens":0}}}),
            json!({"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"why"}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"signed"}}),
            json!({"type":"content_block_stop","index":0}),
            json!({"type":"content_block_start","index":1,"content_block":{"type":"text","text":"Hel"}}),
            json!({"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"lo"}}),
            json!({"type":"content_block_stop","index":1}),
            json!({"type":"content_block_start","index":2,"content_block":{"type":"tool_use","id":"call_one","name":"city","input":{"city":"Taipei"}}}),
            json!({"type":"content_block_stop","index":2}),
            json!({"type":"content_block_start","index":3,"content_block":{"type":"tool_use","id":"call_two","name":"unit","input":{}}}),
            json!({"type":"content_block_delta","index":3,"delta":{"type":"input_json_delta","partial_json":"{\"unit\":"}}),
            json!({"type":"content_block_delta","index":3,"delta":{"type":"input_json_delta","partial_json":"\"C\"}"}}),
            json!({"type":"content_block_stop","index":3}),
            json!({"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":5}}),
            json!({"type":"message_stop"}),
        ],
        _ => unreachable!(),
    }
}

fn convert(wire: Wire, inbound: Inbound) -> Vec<Value> {
    let mut stream = stream::Stream::new(inbound, wire, true, "model");
    let mut events = Vec::new();
    let mut ended = false;
    for chunk in fixture(wire) {
        let frame = stream.event(chunk).unwrap();
        ended |= frame.observation.outcome.is_some();
        events.extend(frame.events);
    }
    if !ended {
        events.extend(stream.finish().unwrap().events);
    }
    events
}

/// Model a client that joins every delta to an already-created output item. The
/// item id and call id are different namespaces; neither can replace the other.
fn read_responses(events: &[Value]) {
    let mut items = HashMap::new();
    let mut parts = HashSet::new();
    let mut text = HashMap::<String, String>::new();
    let mut done = BTreeMap::new();
    for (sequence, event) in events.iter().enumerate() {
        assert_eq!(event["sequence_number"], sequence as u64, "{event}");
        let kind = event["type"].as_str().unwrap();
        if kind == "response.output_item.added" {
            let index = event["output_index"].as_u64().unwrap();
            let id = event["item"]["id"].as_str().unwrap().to_owned();
            assert!(!id.is_empty());
            assert!(items.insert(index, id.clone()).is_none());
            text.insert(id, String::new());
        } else if kind == "response.output_item.done" {
            let index = event["output_index"].as_u64().unwrap();
            let item = &event["item"];
            let id = item["id"].as_str().unwrap();
            assert_eq!(items.get(&index).map(String::as_str), Some(id));
            let content = match item["type"].as_str().unwrap() {
                "message" => &item["content"][0]["text"],
                "reasoning" => &item["summary"][0]["text"],
                "function_call" => &item["arguments"],
                other => panic!("unexpected item {other}"),
            };
            assert_eq!(text[id], content.as_str().unwrap_or_default());
            assert!(
                done.insert(index, item.clone()).is_none(),
                "item closed twice"
            );
        } else if let Some(index) = event["output_index"].as_u64() {
            let id = event["item_id"].as_str().expect("item_id not found");
            assert_eq!(items.get(&index).map(String::as_str), Some(id), "{event}");
            assert!(
                !done.contains_key(&index),
                "event after item completion: {event}"
            );
            let part = if kind.starts_with("response.reasoning_summary_") {
                Some((index, "summary", event["summary_index"].as_u64().unwrap()))
            } else if event.get("content_index").is_some() {
                Some((index, "content", event["content_index"].as_u64().unwrap()))
            } else {
                None
            };
            if kind.ends_with("_part.added") {
                assert!(parts.insert(part.unwrap()), "part opened twice");
            } else if kind.ends_with("_part.done") {
                assert!(parts.remove(&part.unwrap()), "part not open");
            } else if let Some(part) = part {
                assert!(parts.contains(&part), "event without open part: {event}");
            }
            if kind.ends_with(".delta") {
                text.get_mut(id)
                    .unwrap()
                    .push_str(event["delta"].as_str().unwrap());
            }
        }
    }
    assert!(parts.is_empty());
    assert_eq!(items.len(), done.len());
    let terminal = events.last().unwrap();
    assert_eq!(terminal["type"], "response.completed");
    assert_eq!(
        terminal["response"]["output"],
        json!(done.into_values().collect::<Vec<_>>())
    );
}

#[test]
fn generated_responses_keep_item_identity_and_streamed_content() {
    for wire in [Wire::Gemini, Wire::ChatCompletions, Wire::Messages] {
        let events = convert(wire, Inbound::Responses);
        read_responses(&events);
        let output = events.last().unwrap()["response"]["output"]
            .as_array()
            .unwrap();
        assert_eq!(output[0]["summary"][0]["text"], "why", "{wire:?}");
        assert_eq!(output[1]["content"][0]["text"], "Hello", "{wire:?}");
        for (item, id, arguments) in [
            (&output[2], "call_one", json!({"city":"Taipei"})),
            (&output[3], "call_two", json!({"unit":"C"})),
        ] {
            assert_eq!(item["call_id"], id);
            assert_ne!(item["id"], item["call_id"]);
            assert_eq!(
                serde_json::from_str::<Value>(item["arguments"].as_str().unwrap()).unwrap(),
                arguments
            );
        }
    }
}

#[test]
fn tool_fragments_and_initial_blocks_survive_chat_and_messages_conversion() {
    for wire in [Wire::Gemini, Wire::ChatCompletions, Wire::Messages] {
        for inbound in [Inbound::Chat, Inbound::Messages] {
            let mut collector = Collector::new(inbound);
            for event in convert(wire, inbound) {
                collector.push(event).unwrap();
            }
            let output = collector.finish().unwrap();
            let calls: Vec<(String, Value)> = if inbound == Inbound::Chat {
                assert_eq!(output["choices"][0]["message"]["content"], "Hello");
                assert_eq!(output["choices"][0]["message"]["reasoning_content"], "why");
                output["choices"][0]["message"]["tool_calls"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|call| {
                        (
                            call["id"].as_str().unwrap().to_owned(),
                            serde_json::from_str(call["function"]["arguments"].as_str().unwrap())
                                .unwrap(),
                        )
                    })
                    .collect()
            } else {
                output["content"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|block| block["type"] == "tool_use")
                    .map(|block| {
                        (
                            block["id"].as_str().unwrap().to_owned(),
                            block["input"].clone(),
                        )
                    })
                    .collect()
            };
            assert_eq!(
                calls,
                vec![
                    ("call_one".into(), json!({"city":"Taipei"})),
                    ("call_two".into(), json!({"unit":"C"}))
                ],
                "{wire:?} -> {inbound:?}"
            );
        }
    }
}

#[test]
fn finish_chunks_release_content_before_final_usage() {
    for wire in [Wire::Gemini, Wire::ChatCompletions] {
        for inbound in [Inbound::Responses, Inbound::Chat, Inbound::Messages] {
            let mut stream = stream::Stream::new(inbound, wire, true, "model");
            let (content, usage) = if wire == Wire::Gemini {
                (
                    json!({"response":{"candidates":[{"content":{"parts":[
                    {"thought":true,"text":"thinking"},{"text":"visible"},
                    {"functionCall":{"name":"one","id":"call_one","args":{"city":"Taipei"}}},
                    {"functionCall":{"name":"two","id":"call_two","args":{"unit":"C"}}}
                ]},"finishReason":"STOP"}]}}),
                    json!({"response":{"usageMetadata":{"promptTokenCount":10,"candidatesTokenCount":5}}}),
                )
            } else {
                (
                    json!({"id":"chat_one","model":"model","choices":[{"index":0,"delta":{
                    "reasoning_content":"thinking","content":"visible","tool_calls":[
                        {"index":0,"id":"call_one","type":"function","function":{"name":"one","arguments":"{\"city\":\"Taipei\"}"}},
                        {"index":1,"id":"call_two","type":"function","function":{"name":"two","arguments":"{\"unit\":\"C\"}"}}
                    ]},"finish_reason":"tool_calls"}]}),
                    json!({"id":"chat_one","choices":[],"usage":{"prompt_tokens":10,"completion_tokens":5}}),
                )
            };
            let early = stream.event(content).unwrap();
            assert!(early.observation.outcome.is_none());
            let text = serde_json::to_string(&early.events).unwrap();
            for fragment in ["thinking", "visible", "Taipei", "call_two"] {
                assert!(text.contains(fragment), "{wire:?} -> {inbound:?}: {text}");
            }
            assert!(!text.contains("response.completed") && !text.contains("message_stop"));
            assert!(early.events.iter().all(|e| {
                e["choices"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .all(|c| c["finish_reason"].is_null())
            }));
            let mut collector = Collector::new(inbound);
            for event in early.events {
                collector.push(event).unwrap();
            }
            for event in stream.event(usage).unwrap().events {
                collector.push(event).unwrap();
            }
            let final_frame = stream.finish().unwrap();
            assert_eq!(final_frame.observation.outcome, Some(Outcome::Completed));
            for event in final_frame.events {
                collector.push(event).unwrap();
            }
            let response = collector.finish().unwrap();
            assert!(response.to_string().contains("Taipei"));
        }
    }
}

#[test]
fn gemini_tool_round_trip_preserves_signatures_and_call_result_pairing() {
    for inbound in [Inbound::Responses, Inbound::Chat, Inbound::Messages] {
        let mut stream = stream::Stream::new(inbound, Wire::Gemini, true, "claude-sonnet-4-6");
        let mut collector = Collector::new(inbound);
        for parts in [
            json!([{"thought":true,"text":"signed thought","thoughtSignature":"sig-thought"}]),
            json!([{"thought":true,"text":"before tool"}]),
            json!([
                {"functionCall":{"id":"call_one","name":"city","args":{"city":"Taipei"}},"thoughtSignature":"sig-tool"},
                {"functionCall":{"id":"call_two","name":"unit","args":{"unit":"C"}}}
            ]),
        ] {
            for event in stream
                .event(json!({"response":{"candidates":[{"content":{"parts":parts}}]}}))
                .unwrap()
                .events
            {
                collector.push(event).unwrap();
            }
        }
        for event in stream
            .event(json!({"response":{"candidates":[{"finishReason":"STOP"}]}}))
            .unwrap()
            .events
        {
            collector.push(event).unwrap();
        }
        for event in stream.finish().unwrap().events {
            collector.push(event).unwrap();
        }
        let reply = collector.finish().unwrap();
        let body = match inbound {
            Inbound::Responses => {
                let mut input = vec![json!({"role":"user","content":"weather"})];
                input.extend(reply["output"].as_array().unwrap().clone());
                for (id, result) in [("call_one","sunny"),("call_two","Celsius")] {
                    input.push(json!({"type":"function_call_output","call_id":id,"output":result}));
                }
                gemini::request::from_responses(&json!({"model":"claude-sonnet-4-6","input":input}), &http::HeaderMap::new()).unwrap()
            }
            Inbound::Chat => gemini::request::from_chat(&json!({"model":"claude-sonnet-4-6","messages":[
                {"role":"user","content":"weather"},reply["choices"][0]["message"],
                {"role":"tool","tool_call_id":"call_one","content":"sunny"},
                {"role":"tool","tool_call_id":"call_two","content":"Celsius"}
            ]}), &http::HeaderMap::new()).unwrap(),
            Inbound::Messages => gemini::request::from_messages(&json!({"model":"claude-sonnet-4-6","max_tokens":100,"messages":[
                {"role":"user","content":"weather"},
                {"role":"assistant","content":reply["content"]},
                {"role":"user","content":[{"type":"tool_result","tool_use_id":"call_one","content":"sunny"},
                    {"type":"tool_result","tool_use_id":"call_two","content":"Celsius"}]}
            ]}), &http::HeaderMap::new()).unwrap(),
            _ => unreachable!(),
        };
        let contents = body["request"]["contents"].as_array().unwrap();
        let parts = contents
            .iter()
            .flat_map(|c| c["parts"].as_array().unwrap())
            .collect::<Vec<_>>();
        let signed = parts
            .iter()
            .find(|p| p["thoughtSignature"] == "sig-thought")
            .unwrap();
        assert_eq!(signed["text"], "signed thought", "{inbound:?}");
        let call = parts
            .iter()
            .find(|p| p["functionCall"]["id"] == "call_one")
            .unwrap();
        assert_eq!(call["thoughtSignature"], "sig-tool", "{inbound:?}: {body}");
        for (id, name, result) in [
            ("call_one", "city", "sunny"),
            ("call_two", "unit", "Celsius"),
        ] {
            let output = parts
                .iter()
                .find(|p| p["functionResponse"]["id"] == id)
                .unwrap();
            assert_eq!(output["functionResponse"]["name"], name);
            assert_eq!(output["functionResponse"]["response"]["result"], result);
        }
    }
}

#[test]
fn native_chat_keeps_structured_reasoning_for_the_next_turn() {
    let mut converter = events(Inbound::Chat, Wire::ChatCompletions, false, "model");
    let reasoning = json!([{"type":"reasoning","summary":[],"encrypted_content":"signature"}]);
    let converted = converter
        .event(json!({"choices":[{"delta":{"reasoning":reasoning}}]}))
        .unwrap();
    assert_eq!(converted[0]["choices"][0]["delta"]["reasoning"], reasoning);
}
