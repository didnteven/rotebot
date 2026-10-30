//! Read-only Gmail through Google's regular Gmail API, shown to the rest of the app as two MCP-style
//! tools. Unlike Google's Gmail MCP server (a Workspace Developer Preview), this works with any
//! Google account that can use an OAuth client. Only the `gmail.readonly` scope is ever requested.
use crate::{http, mcp::{McpError, Tool, ToolAnnotations}, oauth::AccessToken};
use base64::Engine;
use reqwest::{Client, Response};
use serde_json::{json, Value};

pub const BASE: &str = "https://gmail.googleapis.com/gmail/v1";
const MAX_THREADS: u64 = 20;
const MAX_MESSAGES: usize = 20;
const MAX_BODY_CHARS: usize = 20_000;
const LIMIT: usize = 8 * 1024 * 1024;

pub fn is_gmail_api(url: &str) -> bool {
    url == BASE || url.starts_with("https://gmail.googleapis.com/gmail/v1/")
}

pub fn tools() -> Vec<Tool> {
    let read_only = || ToolAnnotations { read_only_hint: Some(true), destructive_hint: Some(false) };
    vec![
        Tool {
            name: "search_threads".into(),
            description: Some("Search Gmail threads with Gmail's search syntax (for example newer_than:7d, from:name, subject:word). Returns {"threads": [{id, subject, from, date, snippet, messageCount}]}.".into()),
            input_schema: json!({
                "type": "object", "additionalProperties": false,
                "properties": {
                    "query": {"type": "string", "description": "Gmail search query; empty means the newest threads"},
                    "pageSize": {"type": "integer", "minimum": 1, "maximum": MAX_THREADS, "description": "How many threads to return (default 10)"}
                }
            }),
            annotations: read_only(),
        },
        Tool {
            name: "get_thread".into(),
            description: Some("Read one thread by id: returns {"id", "messages": [{id, from, to, date, subject, body}]} with plain-text bodies.".into()),
            input_schema: json!({
                "type": "object", "additionalProperties": false,
                "properties": {"threadId": {"type": "string", "description": "A thread id from search_threads"}},
                "required": ["threadId"]
            }),
            annotations: read_only(),
        },
    ]
}

fn text_result(value: Value) -> Value {
    json!({"content": [{"type": "text", "text": value.to_string()}], "structuredContent": value})
}
fn error_result(message: &str) -> Value {
    let clean: String = message.chars().filter(|c| !c.is_control()).take(300).collect();
    json!({"isError": true, "content": [{"type": "text", "text": clean}]})
}

async fn get(http: &Client, auth: &AccessToken, url: &str, query: &[(&str, String)]) -> Result<Result<Value, String>, McpError> {
    let mut target = url::Url::parse(url).map_err(|_| McpError::InvalidUrl)?;
    target.query_pairs_mut().extend_pairs(query.iter().map(|(k, v)| (*k, v.as_str())));
    let response: Response = http
        .get(target)
        .bearer_auth(auth.expose())
        .send()
        .await
        .map_err(|_| McpError::Transport)?;
    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(McpError::AuthRequired { resource_metadata_url: None, scope_hint: None });
    }
    let body = http::bytes_limited(response, LIMIT).await?;
    let value: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    if status.is_success() {
        return Ok(Ok(value));
    }
    // Google explains refusals (API not enabled, scope missing, quota) in error.message.
    let why = value["error"]["message"].as_str().unwrap_or("Gmail refused the request");
    Ok(Err(format!("Gmail returned {}: {}", status.as_u16(), why)))
}

fn header(message: &Value, name: &str) -> String {
    message["payload"]["headers"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|h| h["name"].as_str().is_some_and(|n| n.eq_ignore_ascii_case(name)))
        .and_then(|h| h["value"].as_str())
        .unwrap_or_default()
        .chars()
        .filter(|c| !c.is_control())
        .take(300)
        .collect()
}

fn decode(data: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(data.trim_end_matches('='))
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default()
}

/// Plain-text body: the text/plain part when there is one, otherwise the HTML with tags removed.
fn body_text(part: &Value) -> Option<String> {
    fn find<'a>(part: &'a Value, mime: &str) -> Option<&'a Value> {
        if part["mimeType"] == mime && part["body"]["data"].is_string() {
            return Some(part);
        }
        part["parts"].as_array()?.iter().find_map(|p| find(p, mime))
    }
    if let Some(p) = find(part, "text/plain") {
        return Some(decode(p["body"]["data"].as_str()?));
    }
    let html = decode(find(part, "text/html")?["body"]["data"].as_str()?);
    let mut out = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => { in_tag = false; out.push(' '); }
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    Some(out.split_whitespace().collect::<Vec<_>>().join(" "))
}

pub async fn call(http: &Client, auth: &AccessToken, name: &str, args: &Value) -> Result<Value, McpError> {
    call_at(BASE, http, auth, name, args).await
}

async fn call_at(base: &str, http: &Client, auth: &AccessToken, name: &str, args: &Value) -> Result<Value, McpError> {
    match name {
        "search_threads" => {
            let query = args["query"].as_str().unwrap_or_default();
            if query.chars().count() > 500 {
                return Ok(error_result("query is longer than 500 characters"));
            }
            let size = args["pageSize"].as_u64().unwrap_or(10).clamp(1, MAX_THREADS);
            let list = match get(http, auth, &format!("{base}/users/me/threads"), &[("q", query.to_string()), ("maxResults", size.to_string())]).await? {
                Ok(v) => v,
                Err(message) => return Ok(error_result(&message)),
            };
            let mut threads = Vec::new();
            for item in list["threads"].as_array().into_iter().flatten().take(size as usize) {
                let Some(id) = item["id"].as_str().filter(|id| id.chars().all(|c| c.is_ascii_alphanumeric())) else { continue };
                let url = format!("{base}/users/me/threads/{id}");
                let q = [("format", "metadata".to_string()), ("metadataHeaders", "Subject".into()), ("metadataHeaders", "From".into()), ("metadataHeaders", "Date".into())];
                let thread = match get(http, auth, &url, &q).await? {
                    Ok(v) => v,
                    Err(message) => return Ok(error_result(&message)),
                };
                let messages = thread["messages"].as_array().cloned().unwrap_or_default();
                let first = messages.first().cloned().unwrap_or(Value::Null);
                let last = messages.last().cloned().unwrap_or(Value::Null);
                threads.push(json!({
                    "id": id,
                    "subject": header(&first, "Subject"),
                    "from": header(&first, "From"),
                    "date": header(&last, "Date"),
                    "snippet": item["snippet"].as_str().unwrap_or_default().chars().filter(|c| !c.is_control()).take(300).collect::<String>(),
                    "messageCount": messages.len(),
                }));
            }
            Ok(text_result(json!({"threads": threads})))
        }
        "get_thread" => {
            let Some(id) = args["threadId"].as_str().filter(|id| !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric())) else {
                return Ok(error_result("threadId must be a thread id from search_threads"));
            };
            let thread = match get(http, auth, &format!("{base}/users/me/threads/{id}"), &[("format", "full".to_string())]).await? {
                Ok(v) => v,
                Err(message) => return Ok(error_result(&message)),
            };
            let messages: Vec<Value> = thread["messages"].as_array().into_iter().flatten().take(MAX_MESSAGES).map(|m| {
                let body: String = body_text(&m["payload"]).unwrap_or_default().chars().take(MAX_BODY_CHARS).collect();
                json!({
                    "id": m["id"], "from": header(m, "From"), "to": header(m, "To"),
                    "date": header(m, "Date"), "subject": header(m, "Subject"), "body": body,
                })
            }).collect();
            Ok(text_result(json!({"id": id, "messages": messages})))
        }
        _ => Err(McpError::ToolNotReadOnly),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_gmail_api_address_is_handled_here_and_both_tools_are_read_only() {
        assert!(is_gmail_api("https://gmail.googleapis.com/gmail/v1"));
        assert!(!is_gmail_api("https://gmailmcp.googleapis.com/mcp/v1"));
        assert!(!is_gmail_api("https://gmail.googleapis.com.evil.example/gmail/v1"));
        for tool in tools() {
            assert_eq!(tool.annotations.read_only_hint, Some(true));
            assert_eq!(tool.annotations.destructive_hint, Some(false));
        }
    }

    #[test]
    fn bodies_prefer_plain_text_and_fall_back_to_html_without_tags() {
        let enc = |s: &str| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(s);
        let both = json!({"mimeType": "multipart/alternative", "parts": [
            {"mimeType": "text/html", "body": {"data": enc("<p>html</p>")}},
            {"mimeType": "text/plain", "body": {"data": enc("plain text")}}]});
        assert_eq!(body_text(&both).unwrap(), "plain text");
        let html = json!({"mimeType": "text/html", "body": {"data": enc("<p>Hello <b>there</b></p>")}});
        assert_eq!(body_text(&html).unwrap(), "Hello there");
    }

    #[test]
    fn headers_are_found_case_insensitively_and_stripped_of_control_characters() {
        let m = json!({"payload": {"headers": [{"name": "subject", "value": "Hi\nthere"}]}});
        assert_eq!(header(&m, "Subject"), "Hithere");
        assert_eq!(header(&m, "From"), "");
    }

    async fn fake_gmail() -> String {
        use axum::{extract::{Path, Query}, http::{HeaderMap, StatusCode}, routing::get, Json, Router};
        use std::collections::HashMap;
        let enc = |s: &str| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(s);
        let body = enc("Your voucher code is on its way.");
        let app = Router::new()
            .route("/users/me/threads", get(|headers: HeaderMap, Query(q): Query<HashMap<String, String>>| async move {
                assert_eq!(headers["authorization"], "Bearer tok");
                if q.get("q").map(String::as_str) == Some("forbidden") {
                    return (StatusCode::FORBIDDEN, Json(json!({"error": {"message": "Gmail API has not been used in project 1 before"}})));
                }
                assert_eq!(q["q"], "newer_than:7d");
                assert_eq!(q["maxResults"], "2");
                (StatusCode::OK, Json(json!({"threads": [{"id": "t1", "snippet": "Hello"}, {"id": "bad/../id", "snippet": "skipped"}]})))
            }))
            .route("/users/me/threads/{id}", get(move |Path(id): Path<String>, Query(q): Query<HashMap<String, String>>| {
                let body = body.clone();
                async move {
                    assert_eq!(id, "t1");
                    let message = if q["format"] == "full" {
                        json!({"id": "m1", "payload": {"headers": [{"name": "From", "value": "a@b.c"}, {"name": "Subject", "value": "Voucher"}], "mimeType": "text/plain", "body": {"data": body}}})
                    } else {
                        json!({"id": "m1", "payload": {"headers": [{"name": "From", "value": "a@b.c"}, {"name": "Subject", "value": "Voucher"}, {"name": "Date", "value": "Fri, 30 Oct 2026"}]}})
                    };
                    Json(json!({"id": "t1", "messages": [message]}))
                }
            }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{address}")
    }

    #[tokio::test]
    async fn search_and_read_use_the_token_and_return_clean_data_or_the_services_reason() {
        let base = fake_gmail().await;
        let http = reqwest::Client::new();
        let auth = AccessToken::new("tok");
        let found = call_at(&base, &http, &auth, "search_threads", &json!({"query": "newer_than:7d", "pageSize": 2})).await.unwrap();
        let threads = &found["structuredContent"]["threads"];
        assert_eq!(threads.as_array().unwrap().len(), 1, "an id with odd characters is skipped");
        assert_eq!(threads[0]["subject"], "Voucher");
        assert_eq!(threads[0]["from"], "a@b.c");
        assert_eq!(threads[0]["date"], "Fri, 30 Oct 2026");
        assert_eq!(threads[0]["snippet"], "Hello");
        let read = call_at(&base, &http, &auth, "get_thread", &json!({"threadId": "t1"})).await.unwrap();
        assert_eq!(read["structuredContent"]["messages"][0]["body"], "Your voucher code is on its way.");
        let refused = call_at(&base, &http, &auth, "search_threads", &json!({"query": "forbidden"})).await.unwrap();
        assert_eq!(refused["isError"], true);
        assert!(refused["content"][0]["text"].as_str().unwrap().contains("Gmail API has not been used"));
        let bad = call_at(&base, &http, &auth, "get_thread", &json!({"threadId": "../x"})).await.unwrap();
        assert_eq!(bad["isError"], true);
        assert!(call_at(&base, &http, &auth, "send_message", &json!({})).await.is_err());
    }
}
