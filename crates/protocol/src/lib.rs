//! Bounded NDJSON transport and additive, versioned JSON-RPC 2.0 envelopes.
//! On a framing error, close the transport rather than retrying the same stream.

use macroscope_domain::JobId;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{BufRead, Write};

/// Maximum serialized JSON bytes per frame (LF excluded, CR included).
pub const MAX_MESSAGE_BYTES: usize = 1024 * 1024;
pub const PROTOCOL_VERSION: &str = "macroscope.ipc/v1";

#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    #[error("transport I/O failed")]
    Io(#[from] std::io::Error),
    #[error("message exceeds the 1 MiB limit")]
    TooLarge,
    #[error("transport closed")]
    Eof,
    #[error("transport closed during a frame")]
    Unterminated,
    #[error("invalid JSON frame")]
    InvalidJson(#[from] serde_json::Error),
    #[error("unsupported protocol version")]
    UnsupportedProtocol,
}

/// Read one LF-terminated JSON value. Allocation never exceeds the message cap,
/// even if a peer sends an unlimited stream without a newline.
pub fn read_message(reader: &mut impl BufRead) -> Result<Value, FrameError> {
    let mut line = Vec::with_capacity(MAX_MESSAGE_BYTES);
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return Err(if line.is_empty() {
                FrameError::Eof
            } else {
                FrameError::Unterminated
            });
        }
        let newline = available.iter().position(|&byte| byte == b'\n');
        let count = newline.unwrap_or(available.len());
        if count > MAX_MESSAGE_BYTES - line.len() {
            return Err(FrameError::TooLarge);
        }
        line.extend_from_slice(&available[..count]);
        reader.consume(count + usize::from(newline.is_some()));
        if newline.is_some() {
            return Ok(serde_json::from_slice(&line)?);
        }
    }
}

/// Serialize into a bounded buffer before touching the transport. An oversized
/// value cannot leave a partial frame behind on the wire.
pub fn write_message(writer: &mut impl Write, value: &Value) -> Result<(), FrameError> {
    struct BoundedBuffer(Vec<u8>);
    impl Write for BoundedBuffer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > MAX_MESSAGE_BYTES - self.0.len() {
                return Err(std::io::Error::other("frame limit"));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut buffer = BoundedBuffer(Vec::with_capacity(MAX_MESSAGE_BYTES));
    // Value serialization can only fail here when the bounded writer rejects it.
    serde_json::to_writer(&mut buffer, value).map_err(|_| FrameError::TooLarge)?;
    writer.write_all(&buffer.0)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestId {
    String(String),
    Number(i64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JsonRpcVersion {
    #[serde(rename = "2.0")]
    V2,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RpcRequest {
    pub jsonrpc: JsonRpcVersion,
    pub id: RequestId,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RpcNotification {
    pub jsonrpc: JsonRpcVersion,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RpcSuccess {
    pub jsonrpc: JsonRpcVersion,
    pub id: RequestId,
    pub result: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RpcFailure {
    pub jsonrpc: JsonRpcVersion,
    pub id: Option<RequestId>,
    pub error: RpcError,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i64,
    /// Stable scrubbed summary; never raw model/exception content.
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Envelope<T> {
    pub protocol_version: String,
    pub build_version: String,
    pub request_id: RequestId,
    pub seq: u64,
    pub policy_epoch: u64,
    pub job_id: JobId,
    pub payload: T,
}

impl<T> Envelope<T> {
    pub fn validate_protocol(&self) -> Result<(), FrameError> {
        if self.protocol_version == PROTOCOL_VERSION {
            Ok(())
        } else {
            Err(FrameError::UnsupportedProtocol)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::{BufReader, Cursor, Read};

    #[test]
    fn framed_roundtrip_multiple_messages_and_embedded_newline() {
        let values = [
            json!({"jsonrpc":"2.0","id":1,"method":"ping","params":{"text":"a\nb"}}),
            json!(null),
        ];
        let mut bytes = Vec::new();
        for value in &values {
            write_message(&mut bytes, value).unwrap();
        }
        let mut reader = BufReader::with_capacity(3, Cursor::new(bytes));
        for value in values {
            assert_eq!(read_message(&mut reader).unwrap(), value);
        }
        assert!(matches!(read_message(&mut reader), Err(FrameError::Eof)));
    }

    #[test]
    fn accepts_exact_limit_and_crlf() {
        let value = json!("x".repeat(MAX_MESSAGE_BYTES - 2));
        let mut bytes = Vec::new();
        write_message(&mut bytes, &value).unwrap();
        assert_eq!(bytes.len(), MAX_MESSAGE_BYTES + 1);
        assert_eq!(read_message(&mut bytes.as_slice()).unwrap(), value);
        assert_eq!(read_message(&mut b"{}\r\n".as_slice()).unwrap(), json!({}));
    }

    #[test]
    fn rejects_oversize_with_or_without_newline() {
        for suffix in ["", "\n"] {
            let bytes = format!("{}{}", "x".repeat(MAX_MESSAGE_BYTES + 1), suffix);
            assert!(matches!(
                read_message(&mut BufReader::with_capacity(31, bytes.as_bytes())),
                Err(FrameError::TooLarge)
            ));
        }
        let mut output = Vec::new();
        assert!(matches!(
            write_message(&mut output, &json!("x".repeat(MAX_MESSAGE_BYTES))),
            Err(FrameError::TooLarge)
        ));
        assert!(output.is_empty());
    }

    #[test]
    fn unbounded_peer_is_rejected_after_bounded_read() {
        struct Endless {
            bytes_read: usize,
        }
        impl Read for Endless {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                buf.fill(b'x');
                self.bytes_read += buf.len();
                Ok(buf.len())
            }
        }
        let mut reader = BufReader::with_capacity(1024, Endless { bytes_read: 0 });
        assert!(matches!(
            read_message(&mut reader),
            Err(FrameError::TooLarge)
        ));
        assert!(reader.get_ref().bytes_read <= MAX_MESSAGE_BYTES + 1024);
    }

    #[test]
    fn partial_reads_work_but_eof_mid_frame_is_an_error() {
        assert_eq!(
            read_message(&mut BufReader::with_capacity(1, b"{\"x\":1}\n".as_slice())).unwrap(),
            json!({"x":1})
        );
        for mut bytes in [b"{\"x\":".as_slice(), b"{}".as_slice()] {
            assert!(matches!(
                read_message(&mut bytes),
                Err(FrameError::Unterminated)
            ));
        }
    }

    #[test]
    fn invalid_json_and_invalid_rpc_version_are_rejected() {
        assert!(matches!(
            read_message(&mut b"{bad}\n".as_slice()),
            Err(FrameError::InvalidJson(_))
        ));
        assert!(serde_json::from_value::<RpcRequest>(
            json!({"jsonrpc":"1.0", "id":1, "method":"ping"})
        )
        .is_err());
    }

    #[test]
    fn additive_fields_are_tolerated_but_protocol_changes_are_not() {
        let mut envelope: Envelope<Value> = serde_json::from_value(json!({
            "protocol_version": PROTOCOL_VERSION, "build_version":"0.1.0",
            "request_id":"request-1", "seq":1, "policy_epoch":2, "job_id":"job-1",
            "payload":{}, "future_field":true
        }))
        .unwrap();
        envelope.validate_protocol().unwrap();
        envelope.protocol_version = "macroscope.ipc/v99".into();
        assert!(matches!(
            envelope.validate_protocol(),
            Err(FrameError::UnsupportedProtocol)
        ));
        let request: RpcRequest = serde_json::from_value(
            json!({"jsonrpc":"2.0", "id":1, "method":"ping", "future":true}),
        )
        .unwrap();
        assert_eq!(request.method, "ping");
    }
}
