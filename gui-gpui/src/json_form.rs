//! Firmware-tool schema forms: references, variants, defaults and validation.
use serde_json::{Value, json};
#[derive(Clone, Debug)]
pub struct SchemaField {
    pub path: String,
    pub label: String,
    pub kind: String,
    pub choices: Vec<Value>,
    pub value: Value,
    pub schema: Value,
}
fn resolve<'a>(root: &'a Value, mut schema: &'a Value) -> &'a Value {
    for _ in 0..24 {
        let Some(reference) = schema["$ref"].as_str().and_then(|s| s.strip_prefix('#')) else {
            break;
        };
        let Some(next) = root.pointer(reference) else {
            break;
        };
        schema = next;
    }
    schema
}
fn matches(root: &Value, schema: &Value, data: &Value) -> bool {
    let schema = resolve(root, schema);
    if let Some(props) = schema["properties"].as_object() {
        props.iter().all(|(k, s)| {
            let s = resolve(root, s);
            s.get("const").is_none_or(|v| v == &data[k])
                && s["enum"].as_array().is_none_or(|a| a.contains(&data[k]))
        })
    } else {
        validate_node(root, schema, data, "", 0).is_ok()
    }
}
fn selected(root: &Value, array: &[Value], data: &Value) -> usize {
    array
        .iter()
        .position(|s| matches(root, s, data))
        .unwrap_or(0)
}
fn defaults(root: &Value, schema: &Value, depth: usize) -> Value {
    if depth > 24 {
        return Value::Null;
    }
    let schema = resolve(root, schema);
    if let Some(v) = schema.get("default").or_else(|| schema.get("const")) {
        return v.clone();
    }
    if let Some(a) = schema["enum"].as_array()
        && let Some(v) = a.first()
    {
        return v.clone();
    }
    if let Some(a) = schema["oneOf"]
        .as_array()
        .or_else(|| schema["anyOf"].as_array())
    {
        return a
            .first()
            .map(|s| defaults(root, s, depth + 1))
            .unwrap_or(Value::Null);
    }
    if let Some(a) = schema["allOf"].as_array() {
        let mut v = json!({});
        for s in a {
            merge(&mut v, defaults(root, s, depth + 1));
        }
        return v;
    }
    match schema["type"].as_str().unwrap_or("object") {
        "string" => json!(""),
        "boolean" => json!(false),
        "integer" | "number" => schema.get("minimum").cloned().unwrap_or(json!(0)),
        "array" => json!([]),
        _ => {
            let mut v = json!({});
            if let Some(p) = schema["properties"].as_object() {
                for (k, s) in p {
                    v[k] = defaults(root, s, depth + 1);
                }
            }
            v
        }
    }
}
fn merge(target: &mut Value, value: Value) {
    if let (Some(t), Some(v)) = (target.as_object_mut(), value.as_object()) {
        for (k, v) in v {
            if let Some(t) = t.get_mut(k) {
                merge(t, v.clone());
            } else {
                t.insert(k.clone(), v.clone());
            }
        }
    } else {
        *target = value;
    }
}
pub fn default_value(schema: &Value) -> Value {
    defaults(schema, schema, 0)
}
pub fn populate(root: &Value, data: &Value) -> Value {
    let mut value = default_value(root);
    merge(&mut value, data.clone());
    value
}
pub fn fields(root: &Value, data: &Value) -> Vec<SchemaField> {
    fn walk(
        root: &Value,
        schema: &Value,
        data: &Value,
        path: &str,
        label: &str,
        out: &mut Vec<SchemaField>,
        depth: usize,
    ) {
        if depth > 24 {
            return;
        }
        let schema = resolve(root, schema);
        let title = schema["title"].as_str().unwrap_or(label);
        if let Some(a) = schema["oneOf"]
            .as_array()
            .or_else(|| schema["anyOf"].as_array())
        {
            let index = selected(root, a, data);
            if a.len() > 1 {
                let choices = a
                    .iter()
                    .enumerate()
                    .map(|(i, s)| {
                        json!(
                            resolve(root, s)["title"]
                                .as_str()
                                .map(str::to_owned)
                                .unwrap_or_else(|| format!("{} {}", title, i + 1))
                        )
                    })
                    .collect();
                let variants: Vec<_> = a.iter().map(|s| defaults(root, s, depth + 1)).collect();
                out.push(SchemaField {
                    path: path.into(),
                    label: title.into(),
                    kind: "variant".into(),
                    choices,
                    value: json!(index),
                    schema: json!({"x-native-variants":variants}),
                });
            }
            if let Some(s) = a.get(index) {
                walk(root, s, data, path, title, out, depth + 1);
            }
            return;
        }
        if let Some(a) = schema["allOf"].as_array() {
            for s in a {
                walk(root, s, data, path, title, out, depth + 1);
            }
            return;
        }
        if let Some(properties) = schema["properties"].as_object() {
            for (key, child) in properties {
                if resolve(root, child).get("const").is_some() {
                    continue;
                }
                let escaped = key.replace('~', "~0").replace('/', "~1");
                walk(
                    root,
                    child,
                    &data[key],
                    &format!("{path}/{escaped}"),
                    key,
                    out,
                    depth + 1,
                );
            }
            return;
        }
        let kind = schema["type"].as_str().unwrap_or(if data.is_object() {
            "object"
        } else if data.is_array() {
            "array"
        } else if data.is_boolean() {
            "boolean"
        } else if data.is_number() {
            "number"
        } else {
            "string"
        });
        let mut schema = schema.clone();
        if kind == "array" {
            schema["items"] = resolve(root, &schema["items"]).clone();
        }
        out.push(SchemaField {
            path: path.into(),
            label: title.into(),
            kind: kind.into(),
            choices: schema["enum"].as_array().cloned().unwrap_or_default(),
            value: data.clone(),
            schema: schema.clone(),
        });
        if kind == "array"
            && let Some(array) = data.as_array()
        {
            for (index, child) in array.iter().enumerate() {
                walk(
                    root,
                    &schema["items"],
                    child,
                    &format!("{path}/{index}"),
                    &format!("{title} {}", index + 1),
                    out,
                    depth + 1,
                );
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, data, "", "Firmware configuration", &mut out, 0);
    out
}
pub fn parse(field: &SchemaField, text: &str) -> Result<Value, String> {
    let value = match field.kind.as_str() {
        "variant" => return Ok(field.value.clone()),
        "boolean" => match text {
            "true" => json!(true),
            "false" => json!(false),
            _ => return Err(format!("{}: invalid boolean", field.label)),
        },
        "integer" => json!(
            text.parse::<i64>()
                .map_err(|_| format!("{}: invalid integer", field.label))?
        ),
        "number" => json!(
            text.parse::<f64>()
                .ok()
                .filter(|n| n.is_finite())
                .ok_or_else(|| format!("{}: invalid number", field.label))?
        ),
        "object" | "array" => {
            serde_json::from_str(text).map_err(|e| format!("{}: {e}", field.label))?
        }
        _ => json!(text),
    };
    validate_node(&field.schema, &field.schema, &value, &field.label, 0)?;
    Ok(value)
}
pub fn validate(schema: &Value, data: &Value) -> Result<(), String> {
    validate_node(schema, schema, data, "firmware", 0)
}
fn validate_node(
    root: &Value,
    schema: &Value,
    data: &Value,
    path: &str,
    depth: usize,
) -> Result<(), String> {
    if depth > 32 {
        return Err(format!("{path}: schema nesting too deep"));
    }
    let schema = resolve(root, schema);
    let error = |s: &str| Err(format!("{path}: {s}"));
    if schema == &json!(false) {
        return error("not allowed");
    }
    if let Some(v) = schema.get("const")
        && v != data
    {
        return error("incorrect constant");
    }
    if let Some(a) = schema["enum"].as_array()
        && !a.contains(data)
    {
        return error("select an available value");
    }
    if let Some(t) = schema.get("type") {
        let types: Vec<_> = t
            .as_array()
            .map(|a| a.iter().filter_map(Value::as_str).collect())
            .unwrap_or_else(|| t.as_str().into_iter().collect());
        if !types.iter().any(|t| match *t {
            "object" => data.is_object(),
            "array" => data.is_array(),
            "string" => data.is_string(),
            "boolean" => data.is_boolean(),
            "integer" => data.as_f64().is_some_and(|n| n.fract() == 0.0),
            "number" => data.is_number(),
            "null" => data.is_null(),
            _ => true,
        }) {
            return error("incorrect type");
        }
    }
    for (key, exact) in [("oneOf", true), ("anyOf", false)] {
        if let Some(a) = schema[key].as_array() {
            let n = a
                .iter()
                .filter(|s| validate_node(root, s, data, path, depth + 1).is_ok())
                .count();
            if n == 0 || (exact && n != 1) {
                return error("configuration does not match the selected variant");
            }
        }
    }
    if let Some(a) = schema["allOf"].as_array() {
        for s in a {
            validate_node(root, s, data, path, depth + 1)?;
        }
    }
    if let Some(props) = schema["properties"].as_object() {
        if let Some(required) = schema["required"].as_array() {
            for k in required.iter().filter_map(Value::as_str) {
                if data.get(k).is_none() {
                    return error(&format!("missing {k}"));
                }
            }
        }
        for (k, s) in props {
            if let Some(v) = data.get(k) {
                validate_node(root, s, v, &format!("{path}/{k}"), depth + 1)?;
            }
        }
        if schema["additionalProperties"] == false
            && data
                .as_object()
                .is_some_and(|o| o.keys().any(|k| !props.contains_key(k)))
        {
            return error("unknown property");
        }
    }
    if let Some(n) = data.as_f64()
        && (!n.is_finite()
            || schema["minimum"].as_f64().is_some_and(|v| n < v)
            || schema["maximum"].as_f64().is_some_and(|v| n > v)
            || schema["exclusiveMinimum"].as_f64().is_some_and(|v| n <= v)
            || schema["exclusiveMaximum"].as_f64().is_some_and(|v| n >= v)
            || schema["multipleOf"]
                .as_f64()
                .is_some_and(|v| v > 0.0 && ((n / v).round() - n / v).abs() > 1e-6))
    {
        return error("outside allowed range");
    }
    if let Some(s) = data.as_str() {
        let len = s.chars().count() as u64;
        if schema["minLength"].as_u64().is_some_and(|v| len < v)
            || schema["maxLength"].as_u64().is_some_and(|v| len > v)
        {
            return error("incorrect text length");
        }
    }
    if let Some(a) = data.as_array() {
        if schema["minItems"]
            .as_u64()
            .is_some_and(|v| (a.len() as u64) < v)
            || schema["maxItems"]
                .as_u64()
                .is_some_and(|v| (a.len() as u64) > v)
        {
            return error("incorrect item count");
        }
        for (i, v) in a.iter().enumerate() {
            if let Some(s) = schema.get("items") {
                validate_node(root, s, v, &format!("{path}/{i}"), depth + 1)?;
            }
        }
    }
    Ok(())
}
