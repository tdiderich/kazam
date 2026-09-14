//! Thin blocking HTTP helpers over ureq 3.
//!
//! ureq 3 turns 4xx/5xx into `Error::StatusCode(u16)` by default and drops the
//! body, but several callers (curata pack fetches) want the body of an error
//! response for the message they show. These helpers disable that behaviour
//! and surface `Status(code, body)` instead, so call sites match on a status
//! code without caring which ureq version is underneath.

use std::fmt;

/// Response bodies larger than this are rejected. Pages, registry listings and
/// images are all well under it; ureq's default (10 MB) is not.
const BODY_LIMIT: u64 = 256 * 1024 * 1024;

#[derive(Debug)]
pub enum Error {
    /// The server answered with a non-2xx status. Carries the body as text
    /// (empty when unreadable).
    Status(u16, String),
    /// Connection, TLS, timeout or body-read failure.
    Transport(ureq::Error),
    /// `send` was asked for an HTTP method it does not support.
    UnsupportedMethod(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Status(code, _) => write!(f, "http status {code}"),
            Error::Transport(e) => write!(f, "{e}"),
            Error::UnsupportedMethod(m) => write!(f, "unsupported HTTP method '{m}'"),
        }
    }
}

impl std::error::Error for Error {}

impl From<ureq::Error> for Error {
    fn from(e: ureq::Error) -> Self {
        match e {
            ureq::Error::StatusCode(code) => Error::Status(code, String::new()),
            other => Error::Transport(other),
        }
    }
}

type Headers<'a> = &'a [(&'a str, &'a str)];

fn get_builder(url: &str, headers: Headers) -> ureq::RequestBuilder<ureq::typestate::WithoutBody> {
    let mut req = ureq::get(url).config().http_status_as_error(false).build();
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    req
}

fn post_builder(url: &str, headers: Headers) -> ureq::RequestBuilder<ureq::typestate::WithBody> {
    let mut req = ureq::post(url).config().http_status_as_error(false).build();
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    req
}

fn text_of(mut resp: ureq::http::Response<ureq::Body>) -> Result<String, Error> {
    let status = resp.status().as_u16();
    let body = resp
        .body_mut()
        .with_config()
        .limit(BODY_LIMIT)
        .read_to_string();
    if (200..300).contains(&status) {
        body.map_err(Error::Transport)
    } else {
        Err(Error::Status(status, body.unwrap_or_default()))
    }
}

/// GET `url` and return the body as text. Non-2xx is `Error::Status`.
pub fn get_text(url: &str, headers: Headers) -> Result<String, Error> {
    text_of(get_builder(url, headers).call()?)
}

/// GET `url` and return the raw body bytes. Non-2xx is `Error::Status`.
pub fn get_bytes(url: &str, headers: Headers) -> Result<Vec<u8>, Error> {
    let mut resp = get_builder(url, headers).call()?;
    let status = resp.status().as_u16();
    if !(200..300).contains(&status) {
        let detail = resp
            .body_mut()
            .with_config()
            .limit(BODY_LIMIT)
            .read_to_string()
            .unwrap_or_default();
        return Err(Error::Status(status, detail));
    }
    resp.body_mut()
        .with_config()
        .limit(BODY_LIMIT)
        .read_to_vec()
        .map_err(Error::Transport)
}

/// POST `body` (already serialized) to `url` and return the response text.
/// Non-2xx is `Error::Status` with the response body as detail.
pub fn post_text(url: &str, headers: Headers, body: &str) -> Result<String, Error> {
    text_of(post_builder(url, headers).send(body)?)
}

/// A response with its status kept, for callers that decide per-status
/// (retry on 429, read an error body) rather than treating non-2xx as failure.
pub struct Response {
    pub status: u16,
    pub retry_after: Option<String>,
    pub text: String,
}

/// Send a request with an arbitrary method, headers, query parameters and an
/// optional body. Any status is returned as `Ok(Response)`; only transport and
/// body-read failures are errors.
pub fn send(
    method: &str,
    url: &str,
    headers: Headers,
    query: &[(String, String)],
    body: Option<&str>,
) -> Result<Response, Error> {
    let agent = ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .http_status_as_error(false)
            .build(),
    );
    let mut resp = match method.to_uppercase().as_str() {
        "GET" | "DELETE" => {
            let mut req = if method.eq_ignore_ascii_case("GET") {
                agent.get(url)
            } else {
                agent.delete(url)
            };
            for (k, v) in headers {
                req = req.header(*k, *v);
            }
            for (k, v) in query {
                req = req.query(k, v);
            }
            req.call()?
        }
        "POST" | "PUT" => {
            let mut req = if method.eq_ignore_ascii_case("POST") {
                agent.post(url)
            } else {
                agent.put(url)
            };
            for (k, v) in headers {
                req = req.header(*k, *v);
            }
            for (k, v) in query {
                req = req.query(k, v);
            }
            match body {
                Some(b) => req.send(b)?,
                None => req.send_empty()?,
            }
        }
        other => return Err(Error::UnsupportedMethod(other.to_string())),
    };
    let status = resp.status().as_u16();
    let retry_after = resp
        .headers()
        .get("Retry-After")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let text = resp
        .body_mut()
        .with_config()
        .limit(BODY_LIMIT)
        .read_to_string()
        .map_err(Error::Transport)?;
    Ok(Response {
        status,
        retry_after,
        text,
    })
}
