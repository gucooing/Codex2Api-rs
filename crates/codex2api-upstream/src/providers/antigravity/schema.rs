//! Function parameter schemas in the subset Cloud Code Assist accepts: references and
//! unions are inlined, constraints move into descriptions, and unknown keywords go.
use serde_json::{Map, Value, json};

const KEEP: [&str; 7] = [
    "type",
    "description",
    "properties",
    "required",
    "items",
    "enum",
    "title",
];
const CONSTRAINTS: [&str; 11] = [
    "minLength",
    "maxLength",
    "pattern",
    "minimum",
    "maximum",
    "multipleOf",
    "exclusiveMinimum",
    "exclusiveMaximum",
    "minItems",
    "maxItems",
    "format",
];

/// Gemini function parameters, or none when the schema declares no properties.
pub(crate) fn parameters(schema: &Value) -> Option<Value> {
    let cleaned = response_schema(schema)?;
    cleaned["properties"]
        .as_object()
        .is_some_and(|p| !p.is_empty())
        .then_some(cleaned)
}

/// A structured-output schema in the same accepted subset.
pub(crate) fn response_schema(schema: &Value) -> Option<Value> {
    let mut schema = schema.as_object()?.clone();
    let mut definitions = Map::new();
    for key in ["$defs", "definitions"] {
        if let Some(Value::Object(defs)) = schema.remove(key) {
            definitions.extend(defs);
        }
    }
    let mut value = Value::Object(schema);
    inline_references(&mut value, &definitions, 0);
    Some(clean(value))
}

fn inline_references(value: &mut Value, definitions: &Map<String, Value>, depth: usize) {
    if depth > 8 {
        return;
    }
    match value {
        Value::Object(object) => {
            if let Some(name) = object.remove("$ref").and_then(|r| {
                r.as_str()
                    .map(|r| r.rsplit('/').next().unwrap_or(r).to_owned())
            }) && let Some(Value::Object(definition)) = definitions.get(&name)
            {
                for (key, item) in definition {
                    object.entry(key.clone()).or_insert_with(|| item.clone());
                }
            }
            for item in object.values_mut() {
                inline_references(item, definitions, depth + 1);
            }
        }
        Value::Array(items) => {
            for item in items {
                inline_references(item, definitions, depth + 1);
            }
        }
        _ => {}
    }
}

fn score(value: &Value) -> u8 {
    let kind = value["type"].as_str().unwrap_or("");
    if value.get("properties").is_some() || kind == "object" {
        3
    } else if value.get("items").is_some() || kind == "array" {
        2
    } else if value.get("enum").is_some()
        || value.get("const").is_some()
        || (!kind.is_empty() && kind != "null")
    {
        1
    } else {
        0
    }
}

fn best(branches: &[Value]) -> Option<Value> {
    branches.iter().max_by_key(|b| score(b)).cloned()
}

fn merge(into: &mut Map<String, Value>, from: &Map<String, Value>) {
    for (key, value) in from {
        match key.as_str() {
            "properties" => {
                let target = into.entry("properties").or_insert_with(|| json!({}));
                if let (Some(target), Some(source)) = (target.as_object_mut(), value.as_object()) {
                    for (name, property) in source {
                        target
                            .entry(name.clone())
                            .or_insert_with(|| property.clone());
                    }
                }
            }
            "required" => {
                let target = into.entry("required").or_insert_with(|| json!([]));
                if let (Some(target), Some(source)) = (target.as_array_mut(), value.as_array()) {
                    for name in source {
                        if !target.contains(name) {
                            target.push(name.clone());
                        }
                    }
                }
            }
            "allOf" | "anyOf" | "oneOf" => {}
            _ => {
                into.entry(key.clone()).or_insert_with(|| value.clone());
            }
        }
    }
}

fn clean(value: Value) -> Value {
    let Value::Object(mut node) = value else {
        return value;
    };
    if let Some(Value::Array(branches)) = node.remove("allOf") {
        for branch in &branches {
            if let Some(branch) = branch.as_object() {
                merge(&mut node, branch);
            }
        }
    }
    let kind = node
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    if kind.is_empty() || kind == "object" {
        for key in ["anyOf", "oneOf"] {
            if let Some(Value::Array(branches)) = node.remove(key)
                && let Some(Value::Object(chosen)) = best(&branches)
            {
                merge(&mut node, &chosen);
            }
        }
    } else {
        node.remove("anyOf");
        node.remove("oneOf");
    }
    if let Some(Value::Array(prefix)) = node.remove("prefixItems")
        && node.get("items").is_none_or(|i| !i.is_object())
    {
        node.insert(
            "items".into(),
            best(&prefix).unwrap_or_else(|| json!({"type":"string"})),
        );
    }
    if let Some(Value::Array(items)) = node.get("items").cloned() {
        node.insert(
            "items".into(),
            best(&items).unwrap_or_else(|| json!({"type":"string"})),
        );
    }
    if let Some(constant) = node.remove("const") {
        node.entry("enum").or_insert_with(|| json!([constant]));
    }
    let hints: Vec<String> = CONSTRAINTS
        .iter()
        .filter_map(|key| node.get(*key).map(|v| format!("{key}: {v}")))
        .collect();
    if !hints.is_empty() {
        let description = node
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        node.insert(
            "description".into(),
            format!("{description} [Constraint: {}]", hints.join(", "))
                .trim()
                .into(),
        );
    }
    node.retain(|key, _| KEEP.contains(&key.as_str()));
    let mut nullable = false;
    let kind = match node.get("type") {
        Some(Value::String(kind)) => {
            let kind = kind.to_ascii_lowercase();
            if kind == "null" {
                nullable = true;
                "string".to_owned()
            } else {
                kind
            }
        }
        Some(Value::Array(kinds)) => {
            let names: Vec<String> = kinds
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_ascii_lowercase)
                .collect();
            nullable = names.iter().any(|k| k == "null");
            names
                .into_iter()
                .find(|k| k != "null")
                .unwrap_or_else(|| "string".into())
        }
        _ if node.contains_key("properties") => "object".into(),
        _ if node.contains_key("items") => "array".into(),
        _ => match node
            .get("enum")
            .and_then(Value::as_array)
            .and_then(|e| e.first())
        {
            Some(Value::Number(n)) if n.is_i64() || n.is_u64() => "integer".into(),
            Some(Value::Number(_)) => "number".into(),
            Some(Value::Bool(_)) => "boolean".into(),
            Some(_) => "string".into(),
            None => "object".into(),
        },
    };
    node.insert("type".into(), kind.clone().into());
    if nullable {
        let description = node
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        if !description.contains("nullable") {
            node.insert(
                "description".into(),
                format!("{description} (nullable)").trim().into(),
            );
        }
    }
    if let Some(Value::Array(values)) = node.get_mut("enum") {
        let mut forced = false;
        for value in values.iter_mut() {
            if !value.is_string() {
                forced = true;
                let text = match &*value {
                    Value::Null => "null".to_owned(),
                    other => other.to_string(),
                };
                *value = text.into();
            }
        }
        if forced {
            node.insert("type".into(), "string".into());
        }
    }
    if kind == "array" {
        let items = node
            .get("items")
            .cloned()
            .filter(|i| i.as_object().is_some_and(|o| !o.is_empty()));
        node.insert(
            "items".into(),
            clean(items.unwrap_or_else(|| json!({"type":"string"}))),
        );
    }
    if let Some(Value::Object(properties)) = node.get_mut("properties") {
        for property in properties.values_mut() {
            *property = clean(std::mem::take(property));
        }
        let names: Vec<String> = properties.keys().cloned().collect();
        if let Some(Value::Array(required)) = node.get_mut("required") {
            required.retain(|name| name.as_str().is_some_and(|n| names.iter().any(|k| k == n)));
            if required.is_empty() {
                node.remove("required");
            }
        }
    } else {
        node.remove("required");
    }
    Value::Object(node)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_unions_constraints_and_unknown_keywords_are_reduced() {
        let schema = json!({
            "$schema":"https://json-schema.org/draft/2020-12/schema",
            "type":"object",
            "additionalProperties":false,
            "$defs":{"Mode":{"type":"string","enum":["fast","slow"]}},
            "properties":{
                "mode":{"$ref":"#/$defs/Mode","description":"speed"},
                "count":{"type":["integer","null"],"minimum":1,"maximum":9},
                "tags":{"type":"array","prefixItems":[{"type":"string"}],"items":false},
                "target":{"anyOf":[{"type":"null"},{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}]},
                "level":{"const":3}
            },
            "required":["mode","missing"]
        });
        let cleaned = parameters(&schema).unwrap();
        assert!(cleaned.get("additionalProperties").is_none());
        assert_eq!(
            cleaned["properties"]["mode"],
            json!({"type":"string","enum":["fast","slow"],"description":"speed"})
        );
        assert_eq!(cleaned["properties"]["count"]["type"], "integer");
        assert!(
            cleaned["properties"]["count"]["description"]
                .as_str()
                .unwrap()
                .contains("minimum: 1")
        );
        assert!(
            cleaned["properties"]["count"]["description"]
                .as_str()
                .unwrap()
                .contains("nullable")
        );
        assert_eq!(
            cleaned["properties"]["tags"]["items"],
            json!({"type":"string"})
        );
        assert_eq!(
            cleaned["properties"]["target"]["properties"]["path"]["type"],
            "string"
        );
        assert_eq!(
            cleaned["properties"]["level"],
            json!({"type":"string","enum":["3"]})
        );
        assert_eq!(cleaned["required"], json!(["mode"]));
        assert_eq!(parameters(&json!({"type":"object","properties":{}})), None);
        assert_eq!(parameters(&json!({"type":"object"})), None);
    }
}
