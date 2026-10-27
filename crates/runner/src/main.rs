use macroscope_runner::{encode_response, execute, Request, Response, REQUEST_CAP};
use std::io::{Read, Write};
fn main() {
    let mut bytes = Vec::new();
    let response = match std::io::stdin()
        .take(REQUEST_CAP as u64 + 1)
        .read_to_end(&mut bytes)
    {
        Ok(_) if bytes.len() <= REQUEST_CAP => match serde_json::from_slice::<Request>(&bytes) {
            Ok(request) => {
                drop(bytes);
                execute(request)
            }
            Err(_) => Response::failure("syntax", "invalid JSON request"),
        },
        _ => Response::failure("forbidden", "stdin request exceeds 20 MB or cannot be read"),
    };
    let bytes = encode_response(&response);
    let _ = std::io::stdout().lock().write_all(&bytes);
}
