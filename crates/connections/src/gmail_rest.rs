//! Gmail through Google's regular Gmail API, shown to the rest of the app as MCP-style tools.
//! Unlike Google's Gmail MCP server (a Workspace Developer Preview), this works with any Google
//! account that can use an OAuth client. Reads run when a tool runs; every change (draft, send,
//! labels, trash) is a planned action the person reviews first, through the effect broker.
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
    let read = || ToolAnnotations { read_only_hint: Some(true), destructive_hint: Some(false) };
    let write = || ToolAnnotations { read_only_hint: Some(false), destructive_hint: Some(false) };
    let message = json!({
        "type": "object", "additionalProperties": false,
        "properties": {
            "to": {"type": "string", "description": "Recipient address(es), comma separated"},
            "cc": {"type": "string"}, "bcc": {"type": "string"},
            "subject": {"type": "string"},
            "body": {"type": "string", "description": "Plain-text body"},
            "threadId": {"type": "string", "description": "Reply inside this thread"},
            "inReplyTo": {"type": "string", "description": "Message-ID header of the message being answered"}
        },
        "required": ["to", "subject", "body"]
    });
    vec![
        Tool {
            name: "search_threads".into(),
            description: Some("Search Gmail threads with Gmail's search syntax (for example newer_than:7d, from:name, subject:word). Returns {\"threads\": [{id, subject, from, date, snippet, messageCount}]}.".into()),
            input_schema: json!({
                "type": "object", "additionalProperties": false,
                "properties": {
                    "query": {"type": "string", "description": "Gmail search query; empty means the newest threads"},
                    "pageSize": {"type": "integer", "minimum": 1, "maximum": MAX_THREADS, "description": "How many threads to return (default 10)"}
                }
            }),
            annotations: read(),
        },
        Tool {
            name: "get_thread".into(),
            description: Some("Read one thread by id: returns {\"id\", \"messages\": [{id, from, to, date, subject, body}]} with plain-text bodies.".into()),
            input_schema: json!({
                "type": "object", "additionalProperties": false,
                "properties": {"threadId": {"type": "string", "description": "A thread id from search_threads"}},
                "required": ["threadId"]
            }),
            annotations: read(),
        },
        Tool {
            name: "list_labels".into(),
            description: Some("List the mailbox's labels: returns {\"labels\": [{id, name}]}. Use the ids with modify_thread.".into()),
            input_schema: json!({"type": "object", "additionalProperties": false, "properties": {}}),
            annotations: read(),
        },
        Tool {
            name: "create_draft".into(),
            description: Some("Save a draft in Gmail. Nothing is sent.".into()),
            input_schema: message.clone(),
            annotations: write(),
        },
        Tool {
            name: "send_message".into(),
            description: Some("Send an email now, or a reply when threadId is given.".into()),
            input_schema: message,
            annotations: write(),
        },
        Tool {
            name: "modify_thread".into(),
            description: Some("Add or remove labels on a thread: archive = remove INBOX, mark read = remove UNREAD, star = add STARRED.".into()),
            input_schema: json!({
                "type": "object", "additionalProperties": false,
                "properties": {
                    "threadId": {"type": "string"},
                    "addLabelIds": {"type": "array", "items": {"type": "string"}},
                    "removeLabelIds": {"type": "array", "items": {"type": "string"}}
                },
                "required": ["threadId"]
            }),
            annotations: write(),
        },
        Tool {
            name: "trash_thread".into(),
            description: Some("Move a thread to Trash (it can be restored from Trash for 30 days).".into()),
            input_schema: json!({
                "type": "object", "additionalProperties": false,
                "properties": {"threadId": {"type": "string"}},
                "required": ["threadId"]
            }),
            annotations: ToolAnnotations { read_only_hint: Some(false), destructive_hint: Some(true) },
        },
    ]
}

/// True for tools that only read; everything else runs only as an approved, reviewed action.
pub fn is_read_only(name: &str) -> bool {
    tools().iter().any(|t| t.name == name && t.annotations.read_only_hint == Some(true))
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

async fn post(http: &Client, auth: &AccessToken, url: &str, body: &Value) -> Result<Result<Value, String>, McpError> {
    let response: Response = http
        .post(url)
        .bearer_auth(auth.expose())
        .json(body)
        .send()
        .await
        .map_err(|_| McpError::Transport)?;
    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(McpError::AuthRequired { resource_metadata_url: None, scope_hint: None });
    }
    let bytes = http::bytes_limited(response, LIMIT).await?;
    let value: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    if status.is_success() {
        return Ok(Ok(value));
    }
    let why = value["error"]["message"].as_str().unwrap_or("Gmail refused the request");
    Ok(Err(format!("Gmail returned {}: {}", status.as_u16(), why)))
}

/// A header value must be one line: a newline would let text smuggle in another header.
fn header_value(value: Option<&str>, required: bool) -> Result<String, String> {
    let v = value.unwrap_or_default().trim();
    if v.chars().any(char::is_control) || v.len() > 998 || (required && v.is_empty()) {
        return Err("an address or subject is empty, too long or contains a line break".into());
    }
    Ok(v.to_string())
}

fn encode_subject(subject: &str) -> String {
    if subject.is_ascii() {
        subject.to_string()
    } else {
        format!("=?UTF-8?B?{}?=", base64::engine::general_purpose::STANDARD.encode(subject))
    }
}

/// The RFC 2822 message Gmail expects, base64url encoded.
fn raw_message(args: &Value) -> Result<String, String> {
    let to = header_value(args["to"].as_str(), true)?;
    let subject = header_value(args["subject"].as_str(), false)?;
    let mut headers = format!("To: {to}\r\n");
    for (key, name) in [("cc", "Cc"), ("bcc", "Bcc")] {
        let v = header_value(args[key].as_str(), false)?;
        if !v.is_empty() {
            headers.push_str(&format!("{name}: {v}\r\n"));
        }
    }
    let reply = header_value(args["inReplyTo"].as_str(), false)?;
    if !reply.is_empty() {
        headers.push_str(&format!("In-Reply-To: {reply}\r\nReferences: {reply}\r\n"));
    }
    let body = args["body"].as_str().unwrap_or_default();
    if body.len() > 200_000 {
        return Err("the body is longer than 200,000 characters".into());
    }
    let wrapped = base64::engine::general_purpose::STANDARD
        .encode(body)
        .as_bytes()
        .chunks(76)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect::<Vec<_>>()
        .join("\r\n");
    let message = format!(
        "{headers}Subject: {}\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=UTF-8\r\nContent-Transfer-Encoding: base64\r\n\r\n{wrapped}\r\n",
        encode_subject(&subject)
    );
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(message))
}

fn thread_id(args: &Value) -> Option<&str> {
    args["threadId"].as_str().filter(|id| !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric()))
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
        "list_labels" => {
            let list = match get(http, auth, &format!("{base}/users/me/labels"), &[]).await? {
                Ok(v) => v,
                Err(message) => return Ok(error_result(&message)),
            };
            let labels: Vec<Value> = list["labels"].as_array().into_iter().flatten()
                .map(|l| json!({"id": l["id"], "name": l["name"]})).collect();
            Ok(text_result(json!({"labels": labels})))
        }
        "create_draft" | "send_message" => {
            let raw = match raw_message(args) {
                Ok(raw) => raw,
                Err(message) => return Ok(error_result(&message)),
            };
            let mut message = json!({"raw": raw});
            if let Some(id) = thread_id(args) {
                message["threadId"] = json!(id);
            }
            let (url, body) = if name == "create_draft" {
                (format!("{base}/users/me/drafts"), json!({"message": message}))
            } else {
                (format!("{base}/users/me/messages/send"), message)
            };
            match post(http, auth, &url, &body).await? {
                Ok(v) => Ok(text_result(json!({"id": v["id"], "threadId": v["threadId"].clone().as_str().map(Value::from).unwrap_or_else(|| v["message"]["threadId"].clone()), "done": if name == "create_draft" { "draft saved" } else { "sent" }}))),
                Err(message) => Ok(error_result(&message)),
            }
        }
        "modify_thread" => {
            let Some(id) = thread_id(args) else { return Ok(error_result("threadId must be a thread id from search_threads")) };
            let labels = |key: &str| -> Vec<String> {
                args[key].as_array().into_iter().flatten().filter_map(Value::as_str)
                    .filter(|l| !l.is_empty() && l.len() <= 100 && l.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | ' ' | '/')))
                    .map(str::to_string).take(20).collect()
            };
            let body = json!({"addLabelIds": labels("addLabelIds"), "removeLabelIds": labels("removeLabelIds")});
            match post(http, auth, &format!("{base}/users/me/threads/{id}/modify"), &body).await? {
                Ok(_) => Ok(text_result(json!({"threadId": id, "done": "labels changed"}))),
                Err(message) => Ok(error_result(&message)),
            }
        }
        "trash_thread" => {
            let Some(id) = thread_id(args) else { return Ok(error_result("threadId must be a thread id from search_threads")) };
            match post(http, auth, &format!("{base}/users/me/threads/{id}/trash"), &json!({})).await? {
                Ok(_) => Ok(text_result(json!({"threadId": id, "done": "moved to Trash"}))),
                Err(message) => Ok(error_result(&message)),
            }
        }
        _ => Err(McpError::ToolNotReadOnly),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_gmail_api_address_is_handled_here_and_only_reads_count_as_read_only() {
        assert!(is_gmail_api("https://gmail.googleapis.com/gmail/v1"));
        assert!(!is_gmail_api("https://gmailmcp.googleapis.com/mcp/v1"));
        assert!(!is_gmail_api("https://gmail.googleapis.com.evil.example/gmail/v1"));
        for name in ["search_threads", "get_thread", "list_labels"] {
            assert!(is_read_only(name), "{name}");
        }
        for name in ["create_draft", "send_message", "modify_thread", "trash_thread", "unknown"] {
            assert!(!is_read_only(name), "{name}");
        }
        assert_eq!(tools().iter().find(|t| t.name == "trash_thread").unwrap().annotations.destructive_hint, Some(true));
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
            .route("/users/me/labels", get(|| async { Json(json!({"labels": [{"id": "INBOX", "name": "INBOX", "type": "system"}]})) }))
            .route("/users/me/drafts", axum::routing::post(|Json(b): Json<Value>| async move {
                let raw = b["message"]["raw"].as_str().unwrap();
                let text = String::from_utf8(base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(raw).unwrap()).unwrap();
                assert!(text.contains("To: a@b.c") && text.contains("Subject: Hi"));
                assert_eq!(b["message"]["threadId"], "t1");
                Json(json!({"id": "d1", "message": {"id": "m1", "threadId": "t1"}}))
            }))
            .route("/users/me/messages/send", axum::routing::post(|Json(b): Json<Value>| async move {
                assert!(b["raw"].is_string() && b.get("threadId").is_none());
                Json(json!({"id": "m9", "threadId": "t9"}))
            }))
            .route("/users/me/threads/{id}/modify", axum::routing::post(|Path(id): Path<String>, Json(b): Json<Value>| async move {
                assert_eq!(id, "t1");
                assert_eq!(b["removeLabelIds"], json!(["INBOX"]));
                assert_eq!(b["addLabelIds"], json!([]), "labels with odd characters are dropped");
                Json(json!({"id": id}))
            }))
            .route("/users/me/threads/{id}/trash", axum::routing::post(|Path(id): Path<String>| async move {
                if id == "nope" {
                    return (StatusCode::FORBIDDEN, Json(json!({"error": {"message": "Insufficient Permission"}})));
                }
                (StatusCode::OK, Json(json!({"id": id})))
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
        assert!(call_at(&base, &http, &auth, "delete_everything", &json!({})).await.is_err());
        let labels = call_at(&base, &http, &auth, "list_labels", &json!({})).await.unwrap();
        assert_eq!(labels["structuredContent"]["labels"][0]["name"], "INBOX");
    }

    #[test]
    fn messages_are_built_safely_and_refuse_header_injection() {
        let raw = raw_message(&json!({"to": "a@b.c", "cc": "d@e.f", "subject": "Café menu", "body": "Hello ☕", "inReplyTo": "<id@x>"})).unwrap();
        let text = String::from_utf8(base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(raw).unwrap()).unwrap();
        assert!(text.contains("To: a@b.c\r\nCc: d@e.f\r\n"));
        assert!(text.contains("In-Reply-To: <id@x>"));
        assert!(text.contains("Subject: =?UTF-8?B?"), "non-ASCII subjects are encoded");
        assert!(!text.contains("Café"));
        for bad in [json!({"to": "a@b.c\r\nBcc: x@y.z", "subject": "s", "body": "b"}), json!({"to": "", "subject": "s", "body": "b"}), json!({"to": "a@b.c", "subject": "s\nBcc: x", "body": "b"})] {
            assert!(raw_message(&bad).is_err(), "{bad}");
        }
    }

    #[tokio::test]
    async fn changes_post_to_the_right_endpoints_with_the_message_and_report_the_services_reason() {
        let base = fake_gmail().await;
        let http = reqwest::Client::new();
        let auth = AccessToken::new("tok");
        let draft = call_at(&base, &http, &auth, "create_draft", &json!({"to": "a@b.c", "subject": "Hi", "body": "Draft body", "threadId": "t1"})).await.unwrap();
        assert_eq!(draft["structuredContent"]["done"], "draft saved");
        assert_eq!(draft["structuredContent"]["id"], "d1");
        let sent = call_at(&base, &http, &auth, "send_message", &json!({"to": "a@b.c", "subject": "Hi", "body": "Sent body"})).await.unwrap();
        assert_eq!(sent["structuredContent"]["done"], "sent");
        assert_eq!(sent["structuredContent"]["id"], "m9");
        let labelled = call_at(&base, &http, &auth, "modify_thread", &json!({"threadId": "t1", "removeLabelIds": ["INBOX"], "addLabelIds": ["bad;label"]})).await.unwrap();
        assert_eq!(labelled["structuredContent"]["done"], "labels changed");
        let trashed = call_at(&base, &http, &auth, "trash_thread", &json!({"threadId": "t1"})).await.unwrap();
        assert_eq!(trashed["structuredContent"]["done"], "moved to Trash");
        let missing = call_at(&base, &http, &auth, "send_message", &json!({"subject": "no recipient", "body": "b"})).await.unwrap();
        assert_eq!(missing["isError"], true);
        let denied = call_at(&base, &http, &auth, "trash_thread", &json!({"threadId": "nope"})).await.unwrap();
        assert!(denied["content"][0]["text"].as_str().unwrap().contains("Insufficient Permission"));
    }
}
