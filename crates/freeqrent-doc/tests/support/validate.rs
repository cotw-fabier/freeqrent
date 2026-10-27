//! A JSON Schema validator for the keywords the generated schema uses:
//! `$ref` (local), `type`, `enum`, `const`, `properties`, `required`,
//! `additionalProperties`, `propertyNames`, `items`, `minItems`,
//! `maxItems`, `minLength`, `pattern` (only the base64 one), `minimum`,
//! `maximum` and `oneOf`. Any other keyword fails loudly, so the schema
//! can't grow past what's tested.

use freeqrent_doc::json::Value;

const ANNOTATIONS: &[&str] = &[
    "description",
    "$schema",
    "$id",
    "title",
    "x-freeqrent-version",
    "$defs",
    "contentEncoding",
];

fn get<'a>(o: &'a [(String, Value)], key: &str) -> Option<&'a Value> {
    o.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn number(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.parse().ok(),
        _ => None,
    }
}

fn is_type(t: &str, v: &Value) -> bool {
    match (t, v) {
        ("null", Value::Null) => true,
        ("boolean", Value::Bool(_)) => true,
        ("string", Value::String(_)) => true,
        ("array", Value::Array(_)) => true,
        ("object", Value::Object(_)) => true,
        ("number", Value::Number(_)) => true,
        ("integer", Value::Number(n)) => n.parse::<f64>().is_ok_and(|x| x.fract() == 0.0),
        _ => false,
    }
}

/// Every place `value` breaks `schema`, as JSON paths; empty when valid.
pub fn validate(root: &Value, schema: &Value, value: &Value, path: &str) -> Vec<String> {
    let Value::Object(s) = schema else {
        return vec![format!("{path}: schema isn't an object")];
    };
    let mut problems = Vec::new();
    macro_rules! fail {
        ($($arg:tt)*) => {
            problems.push(format!("{path}: {}", format!($($arg)*)))
        };
    }
    for (key, rule) in s {
        match key.as_str() {
            k if ANNOTATIONS.contains(&k) => {}
            "$ref" => {
                let Value::String(r) = rule else {
                    panic!("$ref")
                };
                let name = r.strip_prefix("#/$defs/").expect("local refs only");
                let Value::Object(top) = root else {
                    panic!("root")
                };
                let Some(Value::Object(defs)) = get(top, "$defs") else {
                    panic!("$defs")
                };
                let def = get(defs, name).unwrap_or_else(|| panic!("no $defs/{name}"));
                problems.extend(validate(root, def, value, path));
                return problems;
            }
            "type" => {
                let ok = match rule {
                    Value::String(t) => is_type(t, value),
                    Value::Array(ts) => ts
                        .iter()
                        .any(|t| matches!(t, Value::String(t) if is_type(t, value))),
                    _ => panic!("type"),
                };
                if !ok {
                    fail!("not of type {rule:?}");
                }
            }
            "enum" => {
                let Value::Array(options) = rule else {
                    panic!("enum")
                };
                if !options.contains(value) {
                    fail!("{value:?} isn't one of the names");
                }
            }
            "const" => {
                if rule != value {
                    fail!("{value:?} isn't {rule:?}");
                }
            }
            "properties" | "required" | "additionalProperties" | "propertyNames" => {
                let Value::Object(members) = value else {
                    continue;
                };
                match key.as_str() {
                    "properties" => {
                        let Value::Object(props) = rule else {
                            panic!("properties")
                        };
                        for (k, v) in members {
                            if let Some(sub) = get(props, k) {
                                problems.extend(validate(root, sub, v, &format!("{path}.{k}")));
                            }
                        }
                    }
                    "required" => {
                        let Value::Array(keys) = rule else {
                            panic!("required")
                        };
                        for k in keys {
                            let Value::String(k) = k else {
                                panic!("required")
                            };
                            if get(members, k).is_none() {
                                problems.push(format!("{path}: {k} is required"));
                            }
                        }
                    }
                    "additionalProperties" => {
                        let props = match get(s, "properties") {
                            Some(Value::Object(p)) => &p[..],
                            _ => &[],
                        };
                        for (k, v) in members {
                            if get(props, k).is_some() {
                                continue;
                            }
                            match rule {
                                Value::Bool(false) => {
                                    problems.push(format!("{path}.{k}: not allowed"))
                                }
                                Value::Bool(true) => {}
                                sub => {
                                    problems.extend(validate(root, sub, v, &format!("{path}.{k}")))
                                }
                            }
                        }
                    }
                    _ => {
                        for (k, _) in members {
                            problems.extend(validate(
                                root,
                                rule,
                                &Value::String(k.clone()),
                                &format!("{path}.{k}"),
                            ));
                        }
                    }
                }
            }
            "items" => {
                if let Value::Array(items) = value {
                    for (i, item) in items.iter().enumerate() {
                        problems.extend(validate(root, rule, item, &format!("{path}[{i}]")));
                    }
                }
            }
            "minItems" | "maxItems" => {
                if let Value::Array(items) = value {
                    let n = number(rule).expect("count") as usize;
                    if (key == "minItems" && items.len() < n)
                        || (key == "maxItems" && items.len() > n)
                    {
                        fail!("{} items breaks {key} {n}", items.len());
                    }
                }
            }
            "minLength" => {
                if let Value::String(text) = value
                    && text.chars().count() < number(rule).expect("length") as usize
                {
                    fail!("too short");
                }
            }
            "pattern" => {
                assert_eq!(
                    rule,
                    &Value::String("^[A-Za-z0-9+/]*={0,2}$".into()),
                    "only the base64 pattern"
                );
                if let Value::String(text) = value {
                    let body = text.trim_end_matches('=');
                    if text.len() - body.len() > 2
                        || !body
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/')
                    {
                        fail!("not base64");
                    }
                }
            }
            "minimum" | "maximum" => {
                if let (Some(x), Some(bound)) = (number(value), number(rule))
                    && ((key == "minimum" && x < bound) || (key == "maximum" && x > bound))
                {
                    fail!("{x} breaks {key} {bound}");
                }
            }
            "oneOf" => {
                let Value::Array(options) = rule else {
                    panic!("oneOf")
                };
                let matches = options
                    .iter()
                    .filter(|o| validate(root, o, value, path).is_empty())
                    .count();
                if matches != 1 {
                    fail!("matches {matches} of oneOf's {} options", options.len());
                }
            }
            other => panic!("the validator doesn't know {other:?}"),
        }
    }
    problems
}
