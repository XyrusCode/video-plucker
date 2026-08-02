use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use serde_json::Value;
use tauri::{AppHandle, Emitter};

const PAIRING_PORT: u16 = 19877;

/// A tiny HTTP server that accepts URL submissions from the browser extension.
/// Listens on a fixed localhost port so the extension can discover it without
/// native-messaging complexity.
pub struct PairingServer {
    shutdown: Arc<AtomicBool>,
}

impl PairingServer {
    pub fn start(app: AppHandle) -> Self {
        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_clone = shutdown.clone();

        thread::spawn(move || {
            let addr = format!("127.0.0.1:{PAIRING_PORT}");
            let listener = match TcpListener::bind(&addr) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("[pairing] failed to bind {addr}: {e}");
                    return;
                }
            };
            listener
                .set_nonblocking(true)
                .expect("set_nonblocking on pairing listener");

            eprintln!("[pairing] listening on {addr}");

            loop {
                if shutdown_clone.load(Ordering::SeqCst) {
                    break;
                }

                match listener.accept() {
                    Ok((stream, peer)) => {
                        let app = app.clone();
                        thread::spawn(move || {
                            if let Err(e) = handle_connection(stream, &app) {
                                eprintln!("[pairing] error from {peer}: {e}");
                            }
                        });
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(std::time::Duration::from_millis(100));
                    }
                    Err(e) => {
                        eprintln!("[pairing] accept error: {e}");
                        thread::sleep(std::time::Duration::from_millis(100));
                    }
                }
            }
        });

        Self { shutdown }
    }

    /// Signal the server to stop. The thread will exit on its next loop
    /// iteration.
    pub fn stop(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
    }
}

impl Drop for PairingServer {
    fn drop(&mut self) {
        self.stop();
    }
}

fn handle_connection(mut stream: TcpStream, app: &AppHandle) -> std::io::Result<()> {
    let mut reader = BufReader::new(&mut stream);

    // Read the request line.
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let parts: Vec<&str> = request_line.trim().split_whitespace().collect();
    if parts.len() < 2 {
        return send_response(&mut stream, 400, r#"{"error":"bad request"}"#);
    }
    let method = parts[0];
    let path = parts[1];

    // Read headers until the empty line.
    let mut content_length: usize = 0;
    loop {
        let mut header = String::new();
        reader.read_line(&mut header)?;
        let header = header.trim();
        if header.is_empty() {
            break;
        }
        if let Some(val) = header
            .to_lowercase()
            .strip_prefix("content-length:")
            .map(|v| v.trim().parse().unwrap_or(0))
        {
            content_length = val;
        }
    }

    // Read body if present.
    let mut body = String::new();
    if content_length > 0 {
        let mut buf = vec![0u8; content_length];
        reader.read_exact(&mut buf)?;
        body = String::from_utf8_lossy(&buf).to_string();
    }

    match (method, path) {
        ("GET", "/health") => {
            send_response(
                &mut stream,
                200,
                r#"{"status":"ok","app":"Xyrus YT Plucker"}"#,
            )?;
        }
        ("POST", "/pair") => {
            let parsed: Value = match serde_json::from_str(&body) {
                Ok(v) => v,
                Err(e) => {
                    return send_response(
                        &mut stream,
                        400,
                        &format!(r#"{{"error":"invalid json: {e}"}}"#),
                    );
                }
            };

            let url = parsed
                .get("url")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let is_playlist = parsed
                .get("isPlaylist")
                .and_then(Value::as_bool)
                .unwrap_or(false);

            if url.is_empty() {
                return send_response(
                    &mut stream,
                    400,
                    r#"{"error":"url is required"}"#,
                );
            }

            #[derive(Clone, serde::Serialize)]
            #[serde(rename_all = "camelCase")]
            struct UrlReceivedPayload {
                url: String,
                is_playlist: bool,
            }

            let _ = app.emit(
                "pair://url-received",
                UrlReceivedPayload {
                    url,
                    is_playlist,
                },
            );

            send_response(&mut stream, 200, r#"{"status":"ok"}"#)?;
        }
        ("OPTIONS", _) => {
            // CORS preflight — the extension issues these.
            send_response(&mut stream, 204, "")?;
        }
        _ => {
            send_response(&mut stream, 404, r#"{"error":"not found"}"#)?;
        }
    }

    Ok(())
}

fn send_response(stream: &mut TcpStream, status: u16, body: &str) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        404 => "Not Found",
        _ => "Unknown",
    };
    let cors = "Access-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\n";
    let content_type = if body.is_empty() {
        ""
    } else {
        "Content-Type: application/json\r\n"
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\n{cors}{content_type}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}
