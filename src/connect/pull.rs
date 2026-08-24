//! HTTP pull execution (Layer 1 of `connectors/CONNECT_SPEC.md`). Blocking,
//! `ureq`-based - kazam has no async runtime, and none is needed here.

use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::thread::sleep;
use std::time::Duration;

use crate::connect::config::{ConnectorEnv, HostConfig, State};
use crate::connect::transform;
use crate::connect::types::{Auth, Collect, Paginate, Pull, RateLimit, Source};

fn resolve_var(env: &ConnectorEnv, host: &HostConfig, template: &str) -> Result<String> {
    env.resolve(template, host)
}

/// Returns `(header_name, header_value)` to attach to every request for this
/// pull's source.
fn auth_header(auth: &Auth, env: &ConnectorEnv, host: &HostConfig) -> Result<(String, String)> {
    match auth {
        Auth::Bearer { value } => Ok((
            "Authorization".to_string(),
            format!("Bearer {}", resolve_var(env, host, value)?),
        )),
        Auth::ApiKey { header, value } => Ok((header.clone(), resolve_var(env, host, value)?)),
        Auth::Oauth2 {
            token_url,
            client_id,
            client_secret,
            scope,
        } => {
            let token_url = resolve_var(env, host, token_url)?;
            let client_id = resolve_var(env, host, client_id)?;
            let client_secret = resolve_var(env, host, client_secret)?;
            let mut body = serde_json::json!({
                "grant_type": "client_credentials",
                "client_id": client_id,
                "client_secret": client_secret,
            });
            if let Some(scope) = scope {
                body["scope"] = Value::String(scope.clone());
            }
            let resp_text = ureq::post(&token_url)
                .set("Content-Type", "application/json")
                .send_string(&body.to_string())
                .with_context(|| format!("oauth2 token request failed: {}", token_url))?
                .into_string()
                .context("oauth2 token response was not readable")?;
            let resp: Value =
                serde_json::from_str(&resp_text).context("oauth2 token response was not JSON")?;
            let token = resp
                .get("access_token")
                .and_then(|v| v.as_str())
                .context("oauth2 response missing access_token")?;
            Ok(("Authorization".to_string(), format!("Bearer {}", token)))
        }
    }
}

fn get_json_path<'a>(v: &'a Value, path: &str) -> Option<&'a Value> {
    let path = path.trim_start_matches('.');
    if path.is_empty() {
        return Some(v);
    }
    let mut cur = v;
    for seg in path.split('.') {
        cur = cur.get(seg)?;
    }
    Some(cur)
}

fn extract_collection(body: &Value, collect: &Collect) -> Vec<Value> {
    let pointer = match collect {
        Collect::Simple(s) => s.clone(),
        Collect::Conditional { paged, bare } => {
            // Bare responses are the raw array; paged responses wrap it
            // under a key. Guess from the response shape itself.
            if body.is_array() {
                bare.clone()
            } else {
                paged.clone()
            }
        }
    };
    let pointer = pointer.trim_start_matches('.');
    let pointer = pointer.strip_suffix("[]").unwrap_or(pointer);
    let target = if pointer.is_empty() {
        Some(body)
    } else {
        get_json_path(body, pointer)
    };
    match target.and_then(|v| v.as_array()) {
        Some(arr) => arr.clone(),
        None => Vec::new(),
    }
}

fn value_to_query_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        other => other.to_string(),
    }
}

fn upsert_param(params: &mut Vec<(String, String)>, key: &str, value: String) {
    params.retain(|(k, _)| k != key);
    params.push((key.to_string(), value));
}

fn replace_last_sync(v: &Value, last_sync: &str) -> Value {
    match v {
        Value::String(s) => Value::String(s.replace("{{last_sync}}", last_sync)),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), replace_last_sync(v, last_sync)))
                .collect(),
        ),
        Value::Array(arr) => Value::Array(
            arr.iter()
                .map(|v| replace_last_sync(v, last_sync))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn retry_max(rl: &RateLimit) -> u32 {
    match rl {
        RateLimit::RetryAfter { max_retries, .. } => *max_retries,
        RateLimit::FixedDelay { .. } => 0,
    }
}

pub struct PullOutcome {
    pub records: Vec<Value>,
}

/// Execute one named pull to completion (following pagination), applying its
/// per-record transforms as pages come in.
pub fn execute_pull(
    name: &str,
    pull: &Pull,
    source: &Source,
    env: &ConnectorEnv,
    host: &HostConfig,
    state: &State,
) -> Result<PullOutcome> {
    let base_url = resolve_var(env, host, &source.base_url)?;
    let (auth_name, auth_value) = auth_header(&source.auth, env, host)?;

    let mut all_records = Vec::new();
    let mut params: Vec<(String, String)> = pull
        .request
        .params
        .iter()
        .map(|(k, v)| (k.clone(), value_to_query_string(v)))
        .collect();
    let mut body = pull.request.body.clone();
    let last_sync = state
        .last_sync
        .clone()
        .unwrap_or_else(|| (chrono::Utc::now() - chrono::Duration::days(90)).to_rfc3339());

    let retries_total = pull.rate_limit.as_ref().map(retry_max).unwrap_or(0);
    let mut retries_left = retries_total;
    let mut page_count = 0usize;
    let mut offset_value: i64 = match &pull.paginate {
        Some(Paginate::Offset { start, .. }) => *start,
        _ => 0,
    };

    loop {
        page_count += 1;
        if page_count > 1000 {
            bail!(
                "pull '{}' exceeded 1000 pages - aborting (likely infinite pagination)",
                name
            );
        }

        if let Some(RateLimit::FixedDelay { delay_ms }) = &pull.rate_limit {
            if page_count > 1 {
                sleep(Duration::from_millis(*delay_ms));
            }
        }

        let url = format!("{}{}", base_url.trim_end_matches('/'), pull.request.path);
        let mut req = match pull.request.method.to_uppercase().as_str() {
            "GET" => ureq::get(&url),
            "POST" => ureq::post(&url),
            "PUT" => ureq::put(&url),
            "DELETE" => ureq::delete(&url),
            other => bail!("unsupported HTTP method '{}' for pull '{}'", other, name),
        };
        req = req
            .set(&auth_name, &auth_value)
            .set("User-Agent", "kazam-connect");
        for (k, v) in &params {
            req = req.query(k, v);
        }

        let resolved_body = body.as_ref().map(|b| replace_last_sync(b, &last_sync));

        let attempt = if let Some(b) = &resolved_body {
            req.send_string(&b.to_string())
        } else {
            req.call()
        };

        let response = match attempt {
            Ok(r) => r,
            Err(ureq::Error::Status(429, resp)) if retries_left > 0 => {
                let wait_secs = resp
                    .header("Retry-After")
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or_else(|| 10 * (1 << (retries_total - retries_left)));
                eprintln!(
                    "    429 rate limited, waiting {}s ({} retries left)",
                    wait_secs, retries_left
                );
                sleep(Duration::from_secs(wait_secs));
                retries_left -= 1;
                continue;
            }
            Err(ureq::Error::Status(code, resp)) => {
                let detail = resp.into_string().unwrap_or_default();
                bail!("pull '{}' failed ({}): {}", name, code, detail);
            }
            Err(e) => return Err(e).with_context(|| format!("pull '{}' request failed", name)),
        };

        let response_text = response
            .into_string()
            .with_context(|| format!("pull '{}' response was not readable", name))?;
        let json: Value = serde_json::from_str(&response_text)
            .with_context(|| format!("pull '{}' response was not JSON", name))?;
        let mut records = extract_collection(&json, &pull.collect);

        for t in &pull.transforms {
            for r in records.iter_mut() {
                transform::apply_transform(r, t)?;
            }
        }

        let n = records.len();
        all_records.extend(records);
        retries_left = retries_total; // reset backoff counter once a page succeeds

        match &pull.paginate {
            None => break,
            Some(Paginate::Keyset { next_from, send_as }) => {
                match get_json_path(&json, next_from).cloned() {
                    Some(Value::Null) | None => break,
                    Some(next) => upsert_param(&mut params, send_as, value_to_query_string(&next)),
                }
            }
            Some(Paginate::Cursor { next_from, r#while }) => {
                let should_continue = get_json_path(&json, r#while)
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if !should_continue {
                    break;
                }
                let next = get_json_path(&json, next_from)
                    .cloned()
                    .unwrap_or(Value::Null);
                if let Some(Value::Object(map)) = body.as_mut() {
                    map.insert("cursor".to_string(), next);
                } else {
                    upsert_param(&mut params, "cursor", value_to_query_string(&next));
                }
            }
            Some(Paginate::Offset { param, .. }) => {
                if n == 0 {
                    break;
                }
                offset_value += 1;
                upsert_param(&mut params, param, offset_value.to_string());
            }
        }
    }

    Ok(PullOutcome {
        records: all_records,
    })
}
