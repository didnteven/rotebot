//! Email over IMAP and SMTP with an app password: Gmail, Yahoo, iCloud or any server that speaks
//! them. Shown to the rest of the app as MCP-style tools. Reads run when a tool runs; every change
//! (draft, send, flags, move, trash) is a planned action the person reviews first, through the
//! effect broker. The app password lives only in the secret store and is never logged.
use crate::mcp::{McpError, Tool, ToolAnnotations};
use async_imap::types::{Flag, NameAttribute};
use mail_parser::MimeHeaders;
use base64::Engine;
use futures::TryStreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf},
    net::TcpStream,
    time::timeout,
};
use tokio_rustls::{
    rustls::{pki_types::ServerName, ClientConfig, RootCertStore},
    TlsConnector,
};
use zeroize::Zeroize;

const STEP: Duration = Duration::from_secs(30);
const MAX_RESULTS: u64 = 20;
const MAX_BODY_CHARS: usize = 20_000;
const MAX_FETCH_BYTES: usize = 2_000_000;

/// Server and sign-in details for one mailbox. Debug never prints the password.
#[derive(Clone, Serialize, Deserialize)]
pub struct MailConfig {
    pub email: String,
    pub password: String,
    pub imap_host: String,
    pub imap_port: u16,
    pub smtp_host: String,
    pub smtp_port: u16,
    /// Port 587 style: start in plain text and upgrade. False means TLS from the first byte.
    #[serde(default)]
    pub smtp_starttls: bool,
    /// Only false for a local test server on 127.0.0.1.
    #[serde(default = "yes")]
    pub tls: bool,
}
fn yes() -> bool {
    true
}
impl std::fmt::Debug for MailConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MailConfig([REDACTED])")
    }
}
impl Drop for MailConfig {
    fn drop(&mut self) {
        self.password.zeroize();
    }
}

/// Plain words for the person; never includes the password or server text beyond a short reason.
#[derive(Debug, thiserror::Error)]
pub enum MailError {
    #[error("The server did not accept that email address and app password.")]
    Auth,
    #[error("Couldn't reach {0}. Check the server name and your connection.")]
    Unreachable(String),
    #[error("The mail server refused that: {0}")]
    Refused(String),
    #[error("{0}")]
    Invalid(String),
}
fn refused(text: impl AsRef<str>) -> MailError {
    MailError::Refused(text.as_ref().chars().filter(|c| !c.is_control()).take(200).collect())
}
fn imap_error(e: async_imap::error::Error) -> MailError {
    use async_imap::error::Error as E;
    match e {
        E::Io(_) | E::ConnectionLost => MailError::Unreachable("the mail server".into()),
        E::No(t) | E::Bad(t) => refused(t),
        _ => refused("unexpected reply"),
    }
}

impl MailConfig {
    pub fn validate(&self) -> Result<(), MailError> {
        let bad = |s: &str| s.is_empty() || s.chars().any(char::is_control);
        let host_ok = |h: &str| {
            !h.is_empty()
                && h.len() <= 253
                && h.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
                && !h.starts_with(['-', '.'])
        };
        if bad(&self.email) || self.email.len() > 254 || self.email.matches('@').count() != 1 || self.email.contains([' ', '<', '>', ',']) {
            return Err(MailError::Invalid("Enter the full email address.".into()));
        }
        if bad(&self.password) || self.password.len() > 256 {
            return Err(MailError::Invalid("Enter the app password.".into()));
        }
        if !host_ok(&self.imap_host) || !host_ok(&self.smtp_host) || self.imap_port == 0 || self.smtp_port == 0 {
            return Err(MailError::Invalid("Enter valid mail server names and ports.".into()));
        }
        if !self.tls && !["127.0.0.1", "localhost"].contains(&self.imap_host.as_str()) {
            return Err(MailError::Invalid("Mail servers must use an encrypted connection.".into()));
        }
        Ok(())
    }
    fn gmail(&self) -> bool {
        self.imap_host == "imap.gmail.com"
    }
}

// ---------- connection ----------

#[derive(Debug)]
enum Conn {
    Tls(Box<tokio_rustls::client::TlsStream<TcpStream>>),
    Plain(TcpStream),
}
impl AsyncRead for Conn {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            Conn::Tls(s) => Pin::new(s).poll_read(cx, buf),
            Conn::Plain(s) => Pin::new(s).poll_read(cx, buf),
        }
    }
}
impl AsyncWrite for Conn {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<std::io::Result<usize>> {
        match self.get_mut() {
            Conn::Tls(s) => Pin::new(s).poll_write(cx, buf),
            Conn::Plain(s) => Pin::new(s).poll_write(cx, buf),
        }
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            Conn::Tls(s) => Pin::new(s).poll_flush(cx),
            Conn::Plain(s) => Pin::new(s).poll_flush(cx),
        }
    }
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            Conn::Tls(s) => Pin::new(s).poll_shutdown(cx),
            Conn::Plain(s) => Pin::new(s).poll_shutdown(cx),
        }
    }
}

fn tls_connector() -> TlsConnector {
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let provider = Arc::new(tokio_rustls::rustls::crypto::ring::default_provider());
    let config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .expect("ring supports the default protocol versions")
        .with_root_certificates(roots)
        .with_no_client_auth();
    TlsConnector::from(Arc::new(config))
}
async fn tcp(host: &str, port: u16) -> Result<TcpStream, MailError> {
    timeout(STEP, TcpStream::connect((host, port)))
        .await
        .map_err(|_| MailError::Unreachable(host.into()))?
        .map_err(|_| MailError::Unreachable(host.into()))
}
async fn upgrade(host: &str, stream: TcpStream) -> Result<Conn, MailError> {
    let name = ServerName::try_from(host.to_string()).map_err(|_| MailError::Unreachable(host.into()))?;
    let tls = timeout(STEP, tls_connector().connect(name, stream))
        .await
        .map_err(|_| MailError::Unreachable(host.into()))?
        .map_err(|_| refused(format!("{host} did not present a trusted certificate")))?;
    Ok(Conn::Tls(Box::new(tls)))
}
async fn connect(host: &str, port: u16, tls: bool) -> Result<Conn, MailError> {
    let stream = tcp(host, port).await?;
    if tls {
        upgrade(host, stream).await
    } else {
        Ok(Conn::Plain(stream))
    }
}

type Session = async_imap::Session<Conn>;

async fn open(cfg: &MailConfig) -> Result<Session, MailError> {
    cfg.validate()?;
    let conn = connect(&cfg.imap_host, cfg.imap_port, cfg.tls).await?;
    let mut client = async_imap::Client::new(conn);
    timeout(STEP, client.read_response())
        .await
        .map_err(|_| MailError::Unreachable(cfg.imap_host.clone()))?
        .map_err(|_| MailError::Unreachable(cfg.imap_host.clone()))?;
    let login = timeout(STEP, client.login(&cfg.email, &cfg.password))
        .await
        .map_err(|_| MailError::Unreachable(cfg.imap_host.clone()))?;
    match login {
        Ok(session) => Ok(session),
        Err((async_imap::error::Error::No(_), _)) => Err(MailError::Auth),
        Err((e, _)) => Err(imap_error(e)),
    }
}

/// Signs in once and lists folders: proves the address, app password and server all work.
pub async fn verify(cfg: &MailConfig) -> Result<(), MailError> {
    let mut session = open(cfg).await?;
    folders(&mut session).await?;
    let _ = session.logout().await;
    Ok(())
}

struct Folder {
    name: String,
    special: Vec<String>,
}
async fn folders(session: &mut Session) -> Result<Vec<Folder>, MailError> {
    let names: Vec<_> = timeout(STEP, async {
        session.list(Some(""), Some("*")).await.map_err(imap_error)?.try_collect::<Vec<_>>().await.map_err(imap_error)
    })
    .await
    .map_err(|_| MailError::Unreachable("the mail server".into()))??;
    Ok(names
        .iter()
        .filter(|n| !n.attributes().iter().any(|a| matches!(a, NameAttribute::NoSelect)))
        .map(|n| Folder {
            name: n.name().to_string(),
            special: n
                .attributes()
                .iter()
                .filter_map(|a| match a {
                    NameAttribute::Drafts => Some("\\drafts".to_string()),
                    NameAttribute::Trash => Some("\\trash".to_string()),
                    NameAttribute::Sent => Some("\\sent".to_string()),
                    NameAttribute::Archive => Some("\\archive".to_string()),
                    NameAttribute::Junk => Some("\\junk".to_string()),
                    NameAttribute::Extension(e) => Some(e.to_ascii_lowercase()),
                    _ => None,
                })
                .collect(),
        })
        .collect())
}
/// A folder by its role (`\Trash`, `\Drafts`, `\Sent`), falling back to common names.
fn role_folder(all: &[Folder], role: &str, fallbacks: &[&str]) -> Option<String> {
    all.iter()
        .find(|f| f.special.iter().any(|s| s == role))
        .or_else(|| all.iter().find(|f| fallbacks.iter().any(|n| f.name.eq_ignore_ascii_case(n))))
        .map(|f| f.name.clone())
}
fn folder_arg(args: &Value, key: &str) -> Result<String, String> {
    let name = args[key].as_str().unwrap_or("INBOX");
    if name.is_empty() || name.len() > 200 || name.chars().any(char::is_control) {
        return Err(format!("{key} is not a valid folder name"));
    }
    Ok(name.to_string())
}
fn uid_arg(args: &Value) -> Result<String, String> {
    let uid = args["uid"].as_str().map(str::to_string).or_else(|| args["uid"].as_u64().map(|u| u.to_string()));
    uid.filter(|u| !u.is_empty() && u.len() <= 12 && u.chars().all(|c| c.is_ascii_digit()) && u != "0")
        .ok_or_else(|| "uid must be a message uid from search_messages".to_string())
}

// ---------- tools ----------

pub fn tools() -> Vec<Tool> {
    let read = || ToolAnnotations { read_only_hint: Some(true), destructive_hint: Some(false) };
    let write = || ToolAnnotations { read_only_hint: Some(false), destructive_hint: Some(false) };
    let folder = json!({"type": "string", "description": "Folder name (default INBOX)"});
    let uid = json!({"type": "string", "description": "A message uid from search_messages"});
    let message = json!({
        "type": "object", "additionalProperties": false,
        "properties": {
            "to": {"type": "string", "description": "Recipient address(es), comma separated"},
            "cc": {"type": "string"}, "bcc": {"type": "string"},
            "subject": {"type": "string"},
            "body": {"type": "string", "description": "Plain-text body"},
            "inReplyTo": {"type": "string", "description": "Message-ID of the message being answered"}
        },
        "required": ["to", "subject", "body"]
    });
    vec![
        Tool {
            name: "search_messages".into(),
            description: Some("Search a folder, newest first. query words: from:x to:x subject:x since:2026-10-01 before:2026-11-01 newer_than:7d is:unread is:starred; other words search the whole message (Gmail accepts its full search syntax). Returns {\"messages\": [{uid, folder, subject, from, date, unread, messageId}]}.".into()),
            input_schema: json!({"type": "object", "additionalProperties": false, "properties": {
                "query": {"type": "string"}, "folder": folder.clone(),
                "pageSize": {"type": "integer", "minimum": 1, "maximum": MAX_RESULTS}}}),
            annotations: read(),
        },
        Tool {
            name: "get_message".into(),
            description: Some("Read one message: returns {uid, folder, from, to, cc, date, subject, messageId, body, attachments: [{name, size}]} with a plain-text body.".into()),
            input_schema: json!({"type": "object", "additionalProperties": false, "properties": {"uid": uid.clone(), "folder": folder.clone()}, "required": ["uid"]}),
            annotations: read(),
        },
        Tool {
            name: "list_folders".into(),
            description: Some("List the mailbox's folders: returns {\"folders\": [name]}.".into()),
            input_schema: json!({"type": "object", "additionalProperties": false, "properties": {}}),
            annotations: read(),
        },
        Tool { name: "create_draft".into(), description: Some("Save a draft in the Drafts folder. Nothing is sent.".into()), input_schema: message.clone(), annotations: write() },
        Tool { name: "send_message".into(), description: Some("Send an email now.".into()), input_schema: message, annotations: write() },
        Tool {
            name: "mark_message".into(),
            description: Some("Set or clear flags on a message: Seen (read), Flagged (starred), Answered.".into()),
            input_schema: json!({"type": "object", "additionalProperties": false, "properties": {
                "uid": uid.clone(), "folder": folder.clone(),
                "addFlags": {"type": "array", "items": {"type": "string", "enum": ["Seen", "Flagged", "Answered"]}},
                "removeFlags": {"type": "array", "items": {"type": "string", "enum": ["Seen", "Flagged", "Answered"]}}}, "required": ["uid"]}),
            annotations: write(),
        },
        Tool {
            name: "move_message".into(),
            description: Some("Move a message to another folder (archive, a label folder, ...).".into()),
            input_schema: json!({"type": "object", "additionalProperties": false, "properties": {
                "uid": uid.clone(), "folder": folder.clone(), "toFolder": {"type": "string"}}, "required": ["uid", "toFolder"]}),
            annotations: write(),
        },
        Tool {
            name: "trash_message".into(),
            description: Some("Move a message to the Trash folder.".into()),
            input_schema: json!({"type": "object", "additionalProperties": false, "properties": {"uid": uid, "folder": folder}, "required": ["uid"]}),
            annotations: ToolAnnotations { read_only_hint: Some(false), destructive_hint: Some(true) },
        },
    ]
}
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

/// Runs one tool. Refusals the person can act on come back as an error result carrying the reason;
/// only a lost connection or a rejected password is an error to the caller.
pub async fn call(cfg: &MailConfig, name: &str, args: &Value) -> Result<Value, McpError> {
    if !tools().iter().any(|t| t.name == name) {
        return Err(McpError::ToolNotReadOnly);
    }
    match run(cfg, name, args).await {
        Ok(v) => Ok(text_result(v)),
        Err(MailError::Auth) => Err(McpError::AuthRequired { resource_metadata_url: None, scope_hint: None }),
        Err(MailError::Unreachable(_)) => Err(McpError::Transport),
        Err(other) => Ok(error_result(&other.to_string())),
    }
}

async fn run(cfg: &MailConfig, name: &str, args: &Value) -> Result<Value, MailError> {
    let bad = |m: String| MailError::Invalid(m);
    if name == "send_message" {
        let (message, recipients) = build_message(cfg, args, false).map_err(bad)?;
        smtp_send(cfg, &message, &recipients).await?;
        // Gmail files sent mail itself; other servers need a copy placed in Sent.
        if !cfg.gmail() {
            if let Ok(mut session) = open(cfg).await {
                if let Ok(all) = folders(&mut session).await {
                    if let Some(sent) = role_folder(&all, "\\sent", &["Sent", "Sent Items", "Sent Messages"]) {
                        let _ = session.append(&sent, Some("(\\Seen)"), None, &message).await;
                    }
                }
                let _ = session.logout().await;
            }
        }
        return Ok(json!({"done": "sent"}));
    }
    let mut session = open(cfg).await?;
    let result = timeout(Duration::from_secs(90), run_imap(cfg, &mut session, name, args))
        .await
        .map_err(|_| MailError::Unreachable("the mail server".into()))?;
    let _ = session.logout().await;
    result
}

async fn run_imap(cfg: &MailConfig, session: &mut Session, name: &str, args: &Value) -> Result<Value, MailError> {
    let bad = |m: String| MailError::Invalid(m);
    match name {
        "list_folders" => {
            let all = folders(session).await?;
            Ok(json!({"folders": all.iter().map(|f| f.name.clone()).collect::<Vec<_>>()}))
        }
        "search_messages" => {
            let folder = folder_arg(args, "folder").map_err(bad)?;
            let size = args["pageSize"].as_u64().unwrap_or(10).clamp(1, MAX_RESULTS) as usize;
            let criteria = search_criteria(args["query"].as_str().unwrap_or_default(), cfg.gmail(), now_secs()).map_err(bad)?;
            session.examine(&folder).await.map_err(imap_error)?;
            let mut uids: Vec<u32> = session.uid_search(&criteria).await.map_err(imap_error)?.into_iter().collect();
            uids.sort_unstable_by(|a, b| b.cmp(a));
            uids.truncate(size);
            let mut messages = Vec::new();
            if !uids.is_empty() {
                let set = uids.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
                let fetched: Vec<_> = session
                    .uid_fetch(&set, "(UID FLAGS BODY.PEEK[HEADER.FIELDS (FROM SUBJECT DATE MESSAGE-ID)])")
                    .await
                    .map_err(imap_error)?
                    .try_collect()
                    .await
                    .map_err(imap_error)?;
                for f in &fetched {
                    let Some(uid) = f.uid else { continue };
                    let parsed = f.header().and_then(|h| mail_parser::MessageParser::default().parse(h));
                    let unread = !f.flags().any(|fl| matches!(fl, Flag::Seen));
                    messages.push((uid, json!({
                        "uid": uid.to_string(), "folder": folder,
                        "subject": parsed.as_ref().and_then(|m| m.subject()).map(clean_line).unwrap_or_default(),
                        "from": parsed.as_ref().map(|m| from_text(m)).unwrap_or_default(),
                        "date": parsed.as_ref().and_then(|m| m.date()).map(|d| d.to_rfc3339()).unwrap_or_default(),
                        "unread": unread,
                        "messageId": parsed.as_ref().and_then(|m| m.message_id()).map(|id| format!("<{id}>")).unwrap_or_default(),
                    })));
                }
                messages.sort_by(|a, b| b.0.cmp(&a.0));
            }
            Ok(json!({"messages": messages.into_iter().map(|(_, m)| m).collect::<Vec<_>>()}))
        }
        "get_message" => {
            let folder = folder_arg(args, "folder").map_err(bad)?;
            let uid = uid_arg(args).map_err(bad)?;
            session.examine(&folder).await.map_err(imap_error)?;
            let fetched: Vec<_> = session
                .uid_fetch(&uid, format!("(UID BODY.PEEK[]<0.{MAX_FETCH_BYTES}>)"))
                .await
                .map_err(imap_error)?
                .try_collect()
                .await
                .map_err(imap_error)?;
            let raw = fetched.iter().find_map(|f| f.body()).ok_or_else(|| refused("that message no longer exists"))?;
            let m = mail_parser::MessageParser::default().parse(raw).ok_or_else(|| refused("that message could not be read"))?;
            let body: String = m.body_text(0).map(|b| b.to_string()).unwrap_or_default().chars().take(MAX_BODY_CHARS).collect();
            let attachments: Vec<Value> = m.attachments().take(20).map(|a| json!({
                "name": a.attachment_name().map(clean_line).unwrap_or_default(), "size": a.len()})).collect();
            Ok(json!({
                "uid": uid, "folder": folder, "from": from_text(&m), "to": address_text(m.to()), "cc": address_text(m.cc()),
                "date": m.date().map(|d| d.to_rfc3339()).unwrap_or_default(),
                "subject": m.subject().map(clean_line).unwrap_or_default(),
                "messageId": m.message_id().map(|id| format!("<{id}>")).unwrap_or_default(),
                "body": body, "attachments": attachments,
            }))
        }
        "create_draft" => {
            let (message, _) = build_message(cfg, args, true).map_err(bad)?;
            let all = folders(session).await?;
            let drafts = role_folder(&all, "\\drafts", &["Drafts", "[Gmail]/Drafts", "Draft Messages", "Draft"])
                .ok_or_else(|| refused("no Drafts folder was found"))?;
            session.append(&drafts, Some("(\\Draft)"), None, &message).await.map_err(imap_error)?;
            Ok(json!({"done": "draft saved", "folder": drafts}))
        }
        "mark_message" => {
            let folder = folder_arg(args, "folder").map_err(bad)?;
            let uid = uid_arg(args).map_err(bad)?;
            session.select(&folder).await.map_err(imap_error)?;
            for (key, sign) in [("addFlags", "+"), ("removeFlags", "-")] {
                let flags: Vec<&str> = args[key].as_array().into_iter().flatten().filter_map(Value::as_str)
                    .filter(|f| ["Seen", "Flagged", "Answered"].contains(f)).collect();
                if flags.is_empty() {
                    continue;
                }
                let list = flags.iter().map(|f| format!("\\{f}")).collect::<Vec<_>>().join(" ");
                let _: Vec<_> = session.uid_store(&uid, format!("{sign}FLAGS.SILENT ({list})")).await.map_err(imap_error)?
                    .try_collect().await.map_err(imap_error)?;
            }
            Ok(json!({"uid": uid, "done": "flags changed"}))
        }
        "move_message" | "trash_message" => {
            let folder = folder_arg(args, "folder").map_err(bad)?;
            let uid = uid_arg(args).map_err(bad)?;
            let target = if name == "trash_message" {
                let all = folders(session).await?;
                role_folder(&all, "\\trash", &["Trash", "[Gmail]/Trash", "Deleted Items", "Deleted Messages", "Bin"])
                    .ok_or_else(|| refused("no Trash folder was found"))?
            } else {
                folder_arg(args, "toFolder").map_err(bad)?
            };
            session.select(&folder).await.map_err(imap_error)?;
            if session.uid_mv(&uid, &target).await.is_err() {
                // Older servers without MOVE: copy, then remove the original from this folder.
                session.uid_copy(&uid, &target).await.map_err(imap_error)?;
                let _: Vec<_> = session.uid_store(&uid, "+FLAGS.SILENT (\\Deleted)").await.map_err(imap_error)?
                    .try_collect().await.map_err(imap_error)?;
                let _: Vec<_> = session.uid_expunge(&uid).await.map_err(imap_error)?.try_collect().await.map_err(imap_error)?;
            }
            Ok(json!({"uid": uid, "done": if name == "trash_message" { "moved to Trash" } else { "moved" }, "folder": target}))
        }
        _ => Err(bad("unknown tool".into())),
    }
}

fn clean_line(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).take(300).collect()
}
fn address_text(a: Option<&mail_parser::Address<'_>>) -> String {
    a.map(|a| {
        a.iter()
            .map(|x| match (x.name(), x.address()) {
                (Some(n), Some(addr)) => format!("{} <{addr}>", clean_line(n)),
                (None, Some(addr)) => addr.to_string(),
                (Some(n), None) => clean_line(n),
                _ => String::new(),
            })
            .collect::<Vec<_>>()
            .join(", ")
    })
    .unwrap_or_default()
}
fn from_text(m: &mail_parser::Message<'_>) -> String {
    address_text(m.from())
}

// ---------- search ----------

fn now_secs() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}
/// (year, month 1-12, day 1-31) of a day count since 1970-01-01.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}
const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
fn imap_date(secs: i64) -> String {
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    format!("{d}-{}-{y}", MONTHS[m as usize - 1])
}
fn rfc2822_date(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    let rem = secs.rem_euclid(86_400);
    let weekday = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"][days.rem_euclid(7) as usize];
    format!("{weekday}, {d} {} {y} {:02}:{:02}:{:02} +0000", MONTHS[m as usize - 1], rem / 3600, rem % 3600 / 60, rem % 60)
}
fn quoted(s: &str) -> Result<String, String> {
    if s.chars().any(char::is_control) || s.len() > 500 {
        return Err("the search text has a line break or is too long".into());
    }
    Ok(format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")))
}
fn iso_to_imap(s: &str) -> Result<String, String> {
    let p: Vec<&str> = s.split('-').collect();
    let ok = p.len() == 3 && p[0].len() == 4 && p[1].len() == 2 && p[2].len() == 2 && p.iter().all(|x| x.chars().all(|c| c.is_ascii_digit()));
    let (m, d) = (p.get(1).and_then(|x| x.parse::<usize>().ok()).unwrap_or(0), p.get(2).and_then(|x| x.parse::<u32>().ok()).unwrap_or(0));
    if !ok || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return Err("dates look like 2026-10-31".into());
    }
    Ok(format!("{d}-{}-{}", MONTHS[m - 1], p[0]))
}
/// A Gmail-style query as IMAP SEARCH criteria. Gmail itself takes the query as it is.
fn search_criteria(query: &str, gmail: bool, now: i64) -> Result<String, String> {
    let query = query.trim();
    if query.is_empty() {
        return Ok("ALL".into());
    }
    if gmail {
        return Ok(format!("X-GM-RAW {}", quoted(query)?));
    }
    let mut parts = Vec::new();
    let mut words = Vec::new();
    // Split on spaces, keeping "quoted phrases" together.
    let mut current = String::new();
    let mut in_quote = false;
    for c in query.chars() {
        match c {
            '"' => in_quote = !in_quote,
            ' ' if !in_quote => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(c),
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    for word in words {
        let part = match word.split_once(':') {
            Some(("from", v)) if !v.is_empty() => format!("FROM {}", quoted(v)?),
            Some(("to", v)) if !v.is_empty() => format!("TO {}", quoted(v)?),
            Some(("subject", v)) if !v.is_empty() => format!("SUBJECT {}", quoted(v)?),
            Some(("since", v)) => format!("SINCE {}", iso_to_imap(v)?),
            Some(("before", v)) => format!("BEFORE {}", iso_to_imap(v)?),
            Some(("newer_than", v)) => {
                let (n, unit) = v.split_at(v.len().saturating_sub(1));
                let n: i64 = n.parse().map_err(|_| "newer_than looks like 7d, 2w or 1m".to_string())?;
                let days = match unit { "d" => n, "w" => n * 7, "m" => n * 30, _ => return Err("newer_than looks like 7d, 2w or 1m".into()) };
                format!("SINCE {}", imap_date(now - days.clamp(0, 3650) * 86_400))
            }
            Some(("is", "unread")) => "UNSEEN".into(),
            Some(("is", "read")) => "SEEN".into(),
            Some(("is", "starred")) => "FLAGGED".into(),
            _ => format!("TEXT {}", quoted(&word)?),
        };
        parts.push(part);
    }
    Ok(parts.join(" "))
}

// ---------- building and sending ----------

fn header_value(value: Option<&str>, required: bool) -> Result<String, String> {
    let v = value.unwrap_or_default().trim();
    if v.chars().any(char::is_control) || v.len() > 998 || (required && v.is_empty()) {
        return Err("an address or subject is empty, too long or contains a line break".into());
    }
    Ok(v.to_string())
}
/// The bare address inside "Name <a@b.c>" or "a@b.c".
fn bare_address(entry: &str) -> Result<String, String> {
    let inner = match (entry.rfind('<'), entry.rfind('>')) {
        (Some(a), Some(b)) if a < b => &entry[a + 1..b],
        _ => entry,
    }
    .trim();
    let ok = inner.len() <= 254
        && inner.matches('@').count() == 1
        && !inner.starts_with(['@', '-'])
        && !inner.ends_with('@')
        && !inner.chars().any(|c| c.is_whitespace() || c.is_control() || matches!(c, '<' | '>' | '"' | ','));
    ok.then(|| inner.to_string()).ok_or_else(|| format!("“{}” is not a valid email address", clean_line(entry)))
}
fn addresses(list: &str) -> Result<Vec<String>, String> {
    list.split(',').map(str::trim).filter(|s| !s.is_empty()).map(bare_address).collect()
}
fn random_hex() -> String {
    let mut b = [0u8; 12];
    let _ = getrandom::fill(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}
fn encode_subject(subject: &str) -> String {
    if subject.is_ascii() {
        subject.to_string()
    } else {
        format!("=?UTF-8?B?{}?=", base64::engine::general_purpose::STANDARD.encode(subject))
    }
}
/// The RFC 5322 message and the envelope recipients (to, cc and bcc together).
/// A draft keeps its Bcc line; a message that is sent never carries one.
fn build_message(cfg: &MailConfig, args: &Value, keep_bcc: bool) -> Result<(Vec<u8>, Vec<String>), String> {
    let to = header_value(args["to"].as_str(), true)?;
    let cc = header_value(args["cc"].as_str(), false)?;
    let bcc = header_value(args["bcc"].as_str(), false)?;
    let subject = header_value(args["subject"].as_str(), false)?;
    let reply = header_value(args["inReplyTo"].as_str(), false)?;
    let mut recipients = addresses(&to)?;
    if recipients.is_empty() {
        return Err("a recipient is required".into());
    }
    recipients.extend(addresses(&cc)?);
    recipients.extend(addresses(&bcc)?);
    if recipients.len() > 50 {
        return Err("more than 50 recipients".into());
    }
    let body = args["body"].as_str().unwrap_or_default();
    if body.len() > 200_000 {
        return Err("the body is longer than 200,000 characters".into());
    }
    let domain = cfg.email.split('@').nth(1).unwrap_or("localhost");
    let mut head = format!("From: {}\r\nTo: {to}\r\n", cfg.email);
    if !cc.is_empty() {
        head.push_str(&format!("Cc: {cc}\r\n"));
    }
    if keep_bcc && !bcc.is_empty() {
        head.push_str(&format!("Bcc: {bcc}\r\n"));
    }
    if !reply.is_empty() {
        head.push_str(&format!("In-Reply-To: {reply}\r\nReferences: {reply}\r\n"));
    }
    let wrapped = base64::engine::general_purpose::STANDARD
        .encode(body)
        .as_bytes()
        .chunks(76)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect::<Vec<_>>()
        .join("\r\n");
    let message = format!(
        "{head}Subject: {}\r\nDate: {}\r\nMessage-ID: <{}@{domain}>\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=UTF-8\r\nContent-Transfer-Encoding: base64\r\n\r\n{wrapped}\r\n",
        encode_subject(&subject),
        rfc2822_date(now_secs()),
        random_hex(),
    );
    Ok((message.into_bytes(), recipients))
}

/// One SMTP reply: the code and the last line of text.
async fn reply<S: AsyncRead + Unpin>(stream: &mut S) -> Result<(u16, String), MailError> {
    loop {
        let mut line = Vec::new();
        loop {
            let mut byte = [0u8; 1];
            timeout(STEP, stream.read_exact(&mut byte))
                .await
                .map_err(|_| MailError::Unreachable("the mail server".into()))?
                .map_err(|_| MailError::Unreachable("the mail server".into()))?;
            if byte[0] == b'\n' {
                break;
            }
            line.push(byte[0]);
            if line.len() > 2000 {
                return Err(refused("unexpected reply"));
            }
        }
        let text = String::from_utf8_lossy(&line).trim_end().to_string();
        let code: u16 = text.get(..3).and_then(|c| c.parse().ok()).ok_or_else(|| refused("unexpected reply"))?;
        if text.as_bytes().get(3) != Some(&b'-') {
            return Ok((code, text.get(4..).unwrap_or_default().to_string()));
        }
    }
}
async fn command<S: AsyncRead + AsyncWrite + Unpin>(stream: &mut S, line: &str, want: &[u16]) -> Result<(u16, String), MailError> {
    stream.write_all(line.as_bytes()).await.map_err(|_| MailError::Unreachable("the mail server".into()))?;
    stream.write_all(b"\r\n").await.map_err(|_| MailError::Unreachable("the mail server".into()))?;
    let (code, text) = reply(stream).await?;
    if want.contains(&code) {
        Ok((code, text))
    } else if code == 535 || code == 534 || code == 530 {
        Err(MailError::Auth)
    } else {
        Err(refused(format!("{code} {text}")))
    }
}

async fn smtp_send(cfg: &MailConfig, message: &[u8], recipients: &[String]) -> Result<(), MailError> {
    cfg.validate()?;
    let host = &cfg.smtp_host;
    let mut stream = if cfg.smtp_starttls || !cfg.tls {
        let mut plain = Conn::Plain(tcp(host, cfg.smtp_port).await?);
        let (code, _) = reply(&mut plain).await?;
        if code != 220 {
            return Err(refused("the server did not greet us"));
        }
        command(&mut plain, "EHLO macroscope.local", &[250]).await?;
        if cfg.tls {
            command(&mut plain, "STARTTLS", &[220]).await?;
            let Conn::Plain(raw) = plain else { unreachable!() };
            upgrade(host, raw).await?
        } else {
            plain
        }
    } else {
        let mut tls = connect(host, cfg.smtp_port, true).await?;
        let (code, _) = reply(&mut tls).await?;
        if code != 220 {
            return Err(refused("the server did not greet us"));
        }
        tls
    };
    command(&mut stream, "EHLO macroscope.local", &[250]).await?;
    let auth = base64::engine::general_purpose::STANDARD.encode(format!("\0{}\0{}", cfg.email, cfg.password));
    command(&mut stream, &format!("AUTH PLAIN {auth}"), &[235]).await?;
    command(&mut stream, &format!("MAIL FROM:<{}>", cfg.email), &[250]).await?;
    for r in recipients {
        command(&mut stream, &format!("RCPT TO:<{r}>"), &[250, 251]).await?;
    }
    command(&mut stream, "DATA", &[354]).await?;
    // Dot-stuffing: a line that starts with "." would otherwise end the message early.
    let mut data = Vec::with_capacity(message.len() + 8);
    let mut at_line_start = true;
    for &b in message {
        if at_line_start && b == b'.' {
            data.push(b'.');
        }
        data.push(b);
        at_line_start = b == b'\n';
    }
    if !data.ends_with(b"\r\n") {
        data.extend_from_slice(b"\r\n");
    }
    data.extend_from_slice(b".\r\n");
    stream.write_all(&data).await.map_err(|_| MailError::Unreachable("the mail server".into()))?;
    let (code, text) = reply(&mut stream).await?;
    if code != 250 {
        return Err(refused(format!("{code} {text}")));
    }
    let _ = command(&mut stream, "QUIT", &[221]).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> MailConfig {
        MailConfig { email: "me@example.com".into(), password: "secret".into(), imap_host: "imap.example.com".into(), imap_port: 993, smtp_host: "smtp.example.com".into(), smtp_port: 465, smtp_starttls: false, tls: true }
    }

    #[test]
    fn dates_are_computed_without_a_calendar_library() {
        // 2026-10-30 21:00:00 UTC
        let t = 1_793_394_000;
        assert_eq!(imap_date(t), "30-Oct-2026");
        assert_eq!(rfc2822_date(t), "Fri, 30 Oct 2026 21:00:00 +0000");
        assert_eq!(rfc2822_date(0), "Thu, 1 Jan 1970 00:00:00 +0000");
        assert_eq!(imap_date(951_782_400), "29-Feb-2000");
    }

    #[test]
    fn queries_become_safe_imap_criteria() {
        let now = 1_793_394_000;
        assert_eq!(search_criteria("", false, now).unwrap(), "ALL");
        assert_eq!(search_criteria("from:amy is:unread \"big news\"", false, now).unwrap(), "FROM \"amy\" UNSEEN TEXT \"big news\"");
        assert_eq!(search_criteria("newer_than:7d", false, now).unwrap(), "SINCE 23-Oct-2026");
        assert_eq!(search_criteria("since:2026-10-01 before:2026-11-01", false, now).unwrap(), "SINCE 1-Oct-2026 BEFORE 1-Nov-2026");
        assert_eq!(search_criteria("a \"b\" newer_than:2d", true, now).unwrap(), "X-GM-RAW \"a \\\"b\\\" newer_than:2d\"");
        // quotes and backslashes are escaped, line breaks are refused
        assert_eq!(search_criteria("say\\\"x", false, now).unwrap(), "TEXT \"say\\\\x\"");
        assert!(search_criteria("a\r\nb LOGOUT", false, now).is_err());
        assert!(search_criteria("since:soon", false, now).is_err());
    }

    #[test]
    fn built_messages_are_single_header_blocks_with_no_bcc_when_sent() {
        let args = json!({"to": "A <a@b.c>, d@e.f", "bcc": "x@y.z", "subject": "Café", "body": "Hi\n.dot line", "inReplyTo": "<id@x>"});
        let (sent, rcpts) = build_message(&cfg(), &args, false).unwrap();
        let text = String::from_utf8(sent).unwrap();
        assert_eq!(rcpts, ["a@b.c", "d@e.f", "x@y.z"]);
        assert!(!text.contains("Bcc:") && text.contains("From: me@example.com") && text.contains("In-Reply-To: <id@x>"));
        assert!(text.contains("Subject: =?UTF-8?B?") && text.contains("Message-ID: <") && text.contains("@example.com>"));
        let (draft, _) = build_message(&cfg(), &args, true).unwrap();
        assert!(String::from_utf8(draft).unwrap().contains("Bcc: x@y.z"));
        for bad in [json!({"to": "a@b.c\r\nBcc: x@y.z", "subject": "s", "body": "b"}), json!({"to": "", "subject": "s", "body": "b"}),
            json!({"to": "not an address", "subject": "s", "body": "b"}), json!({"to": "a@b.c", "subject": "s\nX: y", "body": "b"})] {
            assert!(build_message(&cfg(), &bad, false).is_err(), "{bad}");
        }
    }

    #[test]
    fn only_reads_are_read_only_and_settings_are_checked() {
        for n in ["search_messages", "get_message", "list_folders"] {
            assert!(is_read_only(n), "{n}");
        }
        for n in ["create_draft", "send_message", "mark_message", "move_message", "trash_message", "nope"] {
            assert!(!is_read_only(n), "{n}");
        }
        assert!(cfg().validate().is_ok());
        let mut c = cfg();
        c.email = "no-at-sign".into();
        assert!(c.validate().is_err());
        let mut c = cfg();
        c.imap_host = "evil host".into();
        assert!(c.validate().is_err());
        let mut c = cfg();
        c.tls = false;
        assert!(c.validate().is_err(), "plain text is only for a local test server");
        assert_eq!(format!("{:?}", cfg()), "MailConfig([REDACTED])");
    }

    #[test]
    fn folders_are_found_by_role_then_by_common_name() {
        let all = vec![
            Folder { name: "INBOX".into(), special: vec![] },
            Folder { name: "[Gmail]/Bin".into(), special: vec!["\\trash".into()] },
            Folder { name: "Drafts".into(), special: vec![] },
        ];
        assert_eq!(role_folder(&all, "\\trash", &["Trash"]).unwrap(), "[Gmail]/Bin");
        assert_eq!(role_folder(&all, "\\drafts", &["Drafts"]).unwrap(), "Drafts");
        assert!(role_folder(&all, "\\sent", &["Sent"]).is_none());
    }

    use std::sync::Mutex;
    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::net::TcpListener;

    #[derive(Default)]
    struct Seen {
        commands: Vec<String>,
        appended: Vec<(String, String)>,
        smtp: Vec<String>,
    }

    const HEADERS: &str = "From: Amy <amy@example.org>\r\nSubject: Voucher \r\nDate: Fri, 30 Oct 2026 09:00:00 +0000\r\nMessage-ID: <abc@example.org>\r\n\r\n";
    const FULL: &str = "From: Amy <amy@example.org>\r\nTo: me@example.com\r\nSubject: Voucher\r\nDate: Fri, 30 Oct 2026 09:00:00 +0000\r\nMessage-ID: <abc@example.org>\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nYour voucher code is ABC123.\r\n";

    /// A tiny IMAP server: enough of the protocol to exercise every tool over a real socket.
    async fn fake_imap(seen: Arc<Mutex<Seen>>) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                let (socket, _) = listener.accept().await.unwrap();
                let seen = seen.clone();
                tokio::spawn(async move {
                    let (r, mut w) = socket.into_split();
                    let mut r = BufReader::new(r);
                    w.write_all(b"* OK ready\r\n").await.unwrap();
                    loop {
                        let mut line = String::new();
                        if r.read_line(&mut line).await.unwrap() == 0 {
                            return;
                        }
                        let line = line.trim_end().to_string();
                        let (tag, rest) = line.split_once(' ').unwrap();
                        seen.lock().unwrap().commands.push(rest.to_string());
                        let up = rest.to_ascii_uppercase();
                        let out = if up.starts_with("LOGIN") {
                            if rest.contains("secret") { format!("{tag} OK logged in\r\n") } else { format!("{tag} NO [AUTHENTICATIONFAILED] bad\r\n") }
                        } else if up.starts_with("LIST") {
                            format!("* LIST (\\HasNoChildren) \"/\" \"INBOX\"\r\n* LIST (\\HasNoChildren \\Drafts) \"/\" \"Drafts\"\r\n* LIST (\\HasNoChildren \\Trash) \"/\" \"Deleted Stuff\"\r\n* LIST (\\HasNoChildren \\Sent) \"/\" \"Sent\"\r\n{tag} OK done\r\n")
                        } else if up.starts_with("EXAMINE") || up.starts_with("SELECT") {
                            format!("* 2 EXISTS\r\n* OK [UIDVALIDITY 1] ok\r\n{tag} OK [READ-ONLY] done\r\n")
                        } else if up.starts_with("UID SEARCH") {
                            format!("* SEARCH 1 2\r\n{tag} OK done\r\n")
                        } else if up.starts_with("UID FETCH") && up.contains("HEADER.FIELDS") {
                            let mut o = String::new();
                            for (i, uid) in [2, 1].iter().enumerate() {
                                let flags = if *uid == 1 { "\\Seen" } else { "" };
                                o += &format!("* {} FETCH (UID {uid} FLAGS ({flags}) BODY[HEADER.FIELDS (FROM SUBJECT DATE MESSAGE-ID)] {{{}}}\r\n{HEADERS})\r\n", i + 1, HEADERS.len());
                            }
                            o + &format!("{tag} OK done\r\n")
                        } else if up.starts_with("UID FETCH") {
                            format!("* 1 FETCH (UID 1 BODY[] {{{}}}\r\n{FULL})\r\n{tag} OK done\r\n", FULL.len())
                        } else if up.starts_with("APPEND") {
                            let size: usize = rest.rsplit('{').next().unwrap().trim_end_matches('}').parse().unwrap();
                            w.write_all(b"+ go ahead\r\n").await.unwrap();
                            let mut data = vec![0u8; size];
                            r.read_exact(&mut data).await.unwrap();
                            let mut end = String::new();
                            r.read_line(&mut end).await.unwrap();
                            let folder = rest.split('"').nth(1).unwrap_or_default().to_string();
                            seen.lock().unwrap().appended.push((folder, String::from_utf8_lossy(&data).into_owned()));
                            format!("{tag} OK appended\r\n")
                        } else if up.starts_with("LOGOUT") {
                            w.write_all(format!("* BYE\r\n{tag} OK bye\r\n").as_bytes()).await.unwrap();
                            return;
                        } else {
                            format!("{tag} OK done\r\n")
                        };
                        w.write_all(out.as_bytes()).await.unwrap();
                    }
                });
            }
        });
        port
    }

    async fn fake_smtp(seen: Arc<Mutex<Seen>>) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                let (socket, _) = listener.accept().await.unwrap();
                let seen = seen.clone();
                tokio::spawn(async move {
                    let (r, mut w) = socket.into_split();
                    let mut r = BufReader::new(r);
                    w.write_all(b"220 hi\r\n").await.unwrap();
                    let mut in_data = false;
                    loop {
                        let mut line = String::new();
                        if r.read_line(&mut line).await.unwrap() == 0 {
                            return;
                        }
                        let line = line.trim_end_matches(['\r', '\n']).to_string();
                        if in_data {
                            if line == "." {
                                in_data = false;
                                w.write_all(b"250 queued\r\n").await.unwrap();
                            } else {
                                seen.lock().unwrap().smtp.push(line);
                            }
                            continue;
                        }
                        seen.lock().unwrap().smtp.push(format!("> {}", if line.starts_with("AUTH") { "AUTH [hidden]".to_string() } else { line.clone() }));
                        let out: &[u8] = if line.starts_with("EHLO") { b"250-hello\r\n250 AUTH PLAIN\r\n" }
                            else if line.starts_with("AUTH") { if line.contains(&base64::engine::general_purpose::STANDARD.encode("\0me@example.com\0secret")) { b"235 ok\r\n" } else { b"535 bad\r\n" } }
                            else if line.starts_with("DATA") { in_data = true; b"354 go\r\n" }
                            else if line.starts_with("QUIT") { w.write_all(b"221 bye\r\n").await.unwrap(); return; }
                            else { b"250 ok\r\n" };
                        w.write_all(out).await.unwrap();
                    }
                });
            }
        });
        port
    }

    fn local(imap: u16, smtp: u16) -> MailConfig {
        MailConfig { email: "me@example.com".into(), password: "secret".into(), imap_host: "127.0.0.1".into(), imap_port: imap, smtp_host: "127.0.0.1".into(), smtp_port: smtp, smtp_starttls: false, tls: false }
    }

    #[tokio::test]
    async fn every_tool_works_against_a_fake_mail_server() {
        let seen = Arc::new(Mutex::new(Seen::default()));
        let cfg = local(fake_imap(seen.clone()).await, fake_smtp(seen.clone()).await);
        verify(&cfg).await.unwrap();
        let mut wrong = cfg.clone();
        wrong.password = "nope".into();
        assert!(matches!(verify(&wrong).await, Err(MailError::Auth)));

        let folders = call(&cfg, "list_folders", &json!({})).await.unwrap();
        assert_eq!(folders["structuredContent"]["folders"], json!(["INBOX", "Drafts", "Deleted Stuff", "Sent"]));

        let found = call(&cfg, "search_messages", &json!({"query": "from:amy", "pageSize": 5})).await.unwrap();
        let messages = found["structuredContent"]["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0]["uid"], "2", "newest first");
        assert_eq!(messages[0]["subject"], "Voucher");
        assert_eq!(messages[0]["from"], "Amy <amy@example.org>");
        assert_eq!(messages[0]["unread"], true);
        assert_eq!(messages[1]["unread"], false);
        assert_eq!(messages[0]["messageId"], "<abc@example.org>");
        assert!(seen.lock().unwrap().commands.iter().any(|c| c == "UID SEARCH FROM \"amy\""));

        let read = call(&cfg, "get_message", &json!({"uid": "1"})).await.unwrap();
        assert_eq!(read["structuredContent"]["body"].as_str().unwrap().trim(), "Your voucher code is ABC123.");
        assert_eq!(read["structuredContent"]["subject"], "Voucher");

        let draft = call(&cfg, "create_draft", &json!({"to": "a@b.c", "subject": "Hello", "body": "Draft text"})).await.unwrap();
        assert_eq!(draft["structuredContent"]["folder"], "Drafts");
        assert_eq!(seen.lock().unwrap().appended[0].0, "Drafts");
        assert!(seen.lock().unwrap().appended[0].1.contains("To: a@b.c"));

        let sent = call(&cfg, "send_message", &json!({"to": "a@b.c", "bcc": "z@y.x", "subject": "Hello", "body": "Body\n.line"})).await.unwrap();
        assert_eq!(sent["structuredContent"]["done"], "sent");
        let smtp = seen.lock().unwrap().smtp.clone();
        assert!(smtp.contains(&"> MAIL FROM:<me@example.com>".to_string()));
        assert!(smtp.contains(&"> RCPT TO:<z@y.x>".to_string()), "bcc recipients get the message");
        assert!(!smtp.iter().any(|l| l.starts_with("Bcc:")), "but the message does not name them");
        assert!(!smtp.iter().any(|l| l.contains("secret")), "the password is never echoed");
        assert!(seen.lock().unwrap().appended.iter().any(|(f, _)| f == "Sent"), "a copy goes to Sent on non-Gmail servers");

        call(&cfg, "mark_message", &json!({"uid": "2", "addFlags": ["Seen", "Bogus"]})).await.unwrap();
        call(&cfg, "trash_message", &json!({"uid": "2"})).await.unwrap();
        call(&cfg, "move_message", &json!({"uid": "2", "toFolder": "Archive"})).await.unwrap();
        let commands = seen.lock().unwrap().commands.clone();
        assert!(commands.contains(&"UID STORE 2 +FLAGS.SILENT (\\Seen)".to_string()), "{commands:?}");
        assert!(commands.contains(&"UID MOVE 2 \"Deleted Stuff\"".to_string()), "trash goes to the folder marked Trash");
        assert!(commands.contains(&"UID MOVE 2 \"Archive\"".to_string()));

        let refused = call(&cfg, "send_message", &json!({"to": "bad address", "subject": "s", "body": "b"})).await.unwrap();
        assert_eq!(refused["isError"], true);
        assert!(call(&cfg, "delete_everything", &json!({})).await.is_err());
        assert!(matches!(call(&wrong, "list_folders", &json!({})).await, Err(McpError::AuthRequired { .. })));
    }
}
