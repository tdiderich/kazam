//! Per-record transform verbs (Layer 2 of `connectors/CONNECT_SPEC.md`).
//! Executed once per pulled record, immediately after pull, before
//! aggregation. All verbs support nested array paths (`.parent[].child`) via
//! [`apply_field`].

use anyhow::Result;
use regex::Regex;
use serde_json::{Map, Value};

use crate::connect::types::Transform;

/// Walk `path` (dot-separated, `[]` suffix means "iterate this array") down
/// into `record`, calling `op` with the parent object map and the leaf key
/// name for every matching location. Mutation happens inside `op` so this
/// works uniformly for a single top-level field or many nested array
/// elements without juggling multiple live `&mut` borrows.
fn walk_apply(v: &mut Value, segs: &[String], op: &mut dyn FnMut(&mut Map<String, Value>, &str)) {
    if segs.is_empty() {
        return;
    }
    if segs.len() == 1 {
        let key = &segs[0];
        if key.ends_with("[]") {
            return;
        }
        if let Some(map) = v.as_object_mut() {
            op(map, key);
        }
        return;
    }
    let seg = &segs[0];
    let rest = &segs[1..];
    if let Some(stripped) = seg.strip_suffix("[]") {
        if let Some(arr) = v.get_mut(stripped).and_then(|x| x.as_array_mut()) {
            for item in arr.iter_mut() {
                walk_apply(item, rest, op);
            }
        }
    } else if let Some(next) = v.get_mut(seg.as_str()) {
        walk_apply(next, rest, op);
    }
}

pub fn apply_field<F>(record: &mut Value, path: &str, mut op: F)
where
    F: FnMut(&mut Map<String, Value>, &str),
{
    let path = path.trim_start_matches('.');
    let segs: Vec<String> = path.split('.').map(|s| s.to_string()).collect();
    walk_apply(record, &segs, &mut op);
}

fn last_segment(path: &str) -> String {
    path.trim_start_matches('.')
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_string()
}

fn is_empty_val(v: Option<&Value>) -> bool {
    match v {
        None => true,
        Some(Value::Null) => true,
        Some(Value::String(s)) => s.is_empty(),
        _ => false,
    }
}

fn is_missing_val(v: Option<&Value>) -> bool {
    matches!(v, None | Some(Value::Null))
}

fn value_as_str(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn coerce_value(v: &Value, to: &str, join: Option<&str>) -> Value {
    match to {
        "list" => match v {
            Value::Array(_) => v.clone(),
            Value::Null => Value::Array(vec![]),
            other => Value::Array(vec![other.clone()]),
        },
        "string" => match v {
            Value::Array(arr) => {
                let sep = join.unwrap_or(", ");
                Value::String(arr.iter().map(value_as_str).collect::<Vec<_>>().join(sep))
            }
            Value::String(_) => v.clone(),
            other => Value::String(value_as_str(other)),
        },
        "int" => match v {
            Value::Number(n) => n
                .as_i64()
                .map(Value::from)
                .or_else(|| n.as_f64().map(|f| Value::from(f as i64)))
                .unwrap_or(Value::Null),
            Value::String(s) => s.trim().parse::<i64>().map(Value::from).unwrap_or(Value::Null),
            other => other.clone(),
        },
        "float" => match v {
            Value::Number(_) => v.clone(),
            Value::String(s) => s
                .trim()
                .parse::<f64>()
                .ok()
                .and_then(serde_json::Number::from_f64)
                .map(Value::Number)
                .unwrap_or(Value::Null),
            other => other.clone(),
        },
        "bool" => match v {
            Value::Bool(_) => v.clone(),
            Value::Number(n) => Value::Bool(n.as_i64() == Some(1)),
            Value::String(s) => {
                let l = s.to_lowercase();
                Value::Bool(matches!(l.as_str(), "true" | "1" | "yes"))
            }
            other => other.clone(),
        },
        _ => v.clone(),
    }
}

fn compute_epoch(v: &Value, to: &str, unit: &str, zero_means: Option<&Value>) -> Value {
    let raw = match v {
        Value::Number(n) => n.as_i64().unwrap_or(0),
        Value::String(s) => s.parse::<i64>().unwrap_or(0),
        _ => 0,
    };
    let seconds = match unit {
        "milliseconds" => raw / 1_000,
        "nanoseconds" => raw / 1_000_000_000,
        _ => raw,
    };
    if seconds == 0 {
        if let Some(zm) = zero_means {
            return zm.clone();
        }
    }
    match to {
        "age_days" => {
            let now = chrono::Utc::now().timestamp();
            Value::from(((now - seconds) / 86_400).max(0))
        }
        _ => match chrono::DateTime::from_timestamp(seconds, 0) {
            Some(dt) => Value::String(dt.to_rfc3339()),
            None => Value::Null,
        },
    }
}

pub fn apply_transform(record: &mut Value, t: &Transform) -> Result<()> {
    match t {
        Transform::Coerce { coerce, to, join } => {
            apply_field(record, coerce, |map, key| {
                if let Some(existing) = map.get(key).cloned() {
                    let coerced = coerce_value(&existing, to, join.as_deref());
                    map.insert(key.to_string(), coerced);
                }
            });
        }
        Transform::Default { default, value, when } => {
            let when = when.as_deref().unwrap_or("empty");
            apply_field(record, default, |map, key| {
                let cur = map.get(key);
                let should = if when == "missing" {
                    is_missing_val(cur)
                } else {
                    is_empty_val(cur)
                };
                if should {
                    map.insert(key.to_string(), value.clone());
                }
            });
        }
        Transform::Rename { rename, to } => {
            let to_key = last_segment(to);
            apply_field(record, rename, move |map, key| {
                if let Some(v) = map.remove(key) {
                    map.insert(to_key.clone(), v);
                }
            });
        }
        Transform::Lowercase { lowercase } => {
            apply_field(record, lowercase, |map, key| {
                if let Some(Value::String(s)) = map.get_mut(key) {
                    *s = s.to_lowercase();
                }
            });
        }
        Transform::Strip {
            strip,
            prefix,
            suffix,
            before_last,
            after_first,
            pattern,
        } => {
            let prefix = prefix.clone();
            let suffix = suffix.clone();
            let before_last = before_last.clone();
            let after_first = after_first.clone();
            let compiled_pattern = pattern.as_deref().and_then(|p| Regex::new(p).ok());
            apply_field(record, strip, move |map, key| {
                if let Some(Value::String(s)) = map.get_mut(key) {
                    if let Some(p) = &prefix {
                        if let Some(rest) = s.strip_prefix(p.as_str()) {
                            *s = rest.to_string();
                        }
                    }
                    if let Some(sf) = &suffix {
                        if let Some(rest) = s.strip_suffix(sf.as_str()) {
                            *s = rest.to_string();
                        }
                    }
                    if let Some(sep) = &before_last {
                        if let Some(idx) = s.rfind(sep.as_str()) {
                            *s = s[idx + sep.len()..].to_string();
                        }
                    }
                    if let Some(sep) = &after_first {
                        if let Some(idx) = s.find(sep.as_str()) {
                            *s = s[..idx].to_string();
                        }
                    }
                    if let Some(re) = &compiled_pattern {
                        *s = re.replace_all(s, "").to_string();
                    }
                }
            });
        }
        Transform::Regex {
            regex,
            pattern,
            capture,
            into,
            default,
        } => {
            let re = Regex::new(pattern)?;
            let mut extracted: Option<String> = None;
            apply_field(record, regex, |map, key| {
                if let Some(Value::String(s)) = map.get(key) {
                    if let Some(caps) = re.captures(s) {
                        extracted = caps.get(*capture).map(|m| m.as_str().to_string());
                    }
                }
            });
            let val = extracted
                .map(Value::String)
                .or_else(|| default.clone())
                .unwrap_or(Value::Null);
            apply_field(record, into, move |map, key| {
                map.insert(key.to_string(), val.clone());
            });
        }
        Transform::Epoch {
            epoch,
            to,
            into,
            zero_means,
            unit,
        } => {
            let unit = unit.clone().unwrap_or_else(|| "seconds".to_string());
            let to = to.clone();
            let zero_means = zero_means.clone();
            let target_key_default = last_segment(epoch);
            let mut result: Option<Value> = None;
            apply_field(record, epoch, |map, key| {
                if let Some(v) = map.get(key) {
                    result = Some(compute_epoch(v, &to, &unit, zero_means.as_ref()));
                }
            });
            if let Some(val) = result {
                let target_path = into.clone().unwrap_or_else(|| format!(".{}", target_key_default));
                apply_field(record, &target_path, move |map, key| {
                    map.insert(key.to_string(), val.clone());
                });
            }
        }
        Transform::Flatten {
            flatten,
            separator,
            prefix,
        } => {
            let sep = separator.clone().unwrap_or_else(|| ".".to_string());
            let mut flat: Vec<(String, Value)> = Vec::new();
            apply_field(record, flatten, |map, key| {
                if let Some(Value::Object(obj)) = map.get(key) {
                    for (k, v) in obj {
                        flat.push((k.clone(), v.clone()));
                    }
                }
            });
            if let Value::Object(top) = record {
                for (k, v) in flat {
                    let name = match &prefix {
                        Some(p) => format!("{}{}{}", p, sep, k),
                        None => k,
                    };
                    top.insert(name, v);
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connect::types::Transform;
    use serde_json::json;

    #[test]
    fn coerce_string_to_list_wraps() {
        let mut r = json!({"service_protocol": "http"});
        apply_transform(
            &mut r,
            &Transform::Coerce {
                coerce: ".service_protocol".into(),
                to: "list".into(),
                join: None,
            },
        )
        .unwrap();
        assert_eq!(r["service_protocol"], json!(["http"]));
    }

    #[test]
    fn strip_nested_array_path() {
        let mut r = json!({"related_scanner_findings": [{"asset_name": "host/eth0"}]});
        apply_transform(
            &mut r,
            &Transform::Strip {
                strip: ".related_scanner_findings[].asset_name".into(),
                prefix: None,
                suffix: None,
                before_last: Some("/".into()),
                after_first: None,
                pattern: None,
            },
        )
        .unwrap();
        assert_eq!(r["related_scanner_findings"][0]["asset_name"], "eth0");
    }

    #[test]
    fn default_fills_missing() {
        let mut r = json!({});
        apply_transform(
            &mut r,
            &Transform::Default {
                default: ".type".into(),
                value: json!("unknown"),
                when: Some("missing".into()),
            },
        )
        .unwrap();
        assert_eq!(r["type"], "unknown");
    }
}
