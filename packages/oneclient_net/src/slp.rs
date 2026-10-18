//! Server List Ping (SLP): the modern handshake every 1.7+ server answers
//! no authentication, no library — just a varint-framed status exchange
//! used by the saved servers page to show live status dots

use std::net::{SocketAddr, ToSocketAddrs};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub const DEFAULT_PORT: u16 = 25565;

#[derive(Debug, thiserror::Error)]
pub enum PingError {
    #[error("'{0}' is not a valid server address")]
    InvalidAddress(String),
    #[error("could not resolve '{0}'")]
    Resolve(String),
    #[error("server did not answer in time")]
    Timeout,
    #[error("server closed the connection")]
    Closed,
    #[error("unexpected answer from the server")]
    Protocol,
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ServerStatus {
    pub players_online: u32,
    pub players_max: u32,
    pub version_name: String,
    pub motd: String,
    pub latency_ms: u64,
}

#[derive(Deserialize)]
struct StatusResponse {
    #[serde(default)]
    version: Option<VersionField>,
    #[serde(default)]
    players: Option<PlayersField>,
    #[serde(default)]
    description: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct VersionField {
    #[serde(default)]
    name: Option<String>,
}

#[derive(Deserialize)]
struct PlayersField {
    #[serde(default)]
    online: u32,
    #[serde(default)]
    max: u32,
}

/// Splits `host`, `host:port` or `[ipv6]:port` into its parts defaulting the
/// port the way every launcher does
pub fn parse_address(input: &str) -> Result<(String, u16), PingError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(PingError::InvalidAddress(input.to_string()));
    }

    if let Some(rest) = input.strip_prefix('[') {
        let Some((host, tail)) = rest.split_once(']') else {
            return Err(PingError::InvalidAddress(input.to_string()));
        };
        let port = match tail.strip_prefix(':') {
            Some(port) => port
                .parse()
                .map_err(|_| PingError::InvalidAddress(input.to_string()))?,
            None if tail.is_empty() => DEFAULT_PORT,
            None => return Err(PingError::InvalidAddress(input.to_string())),
        };
        return Ok((host.to_string(), port));
    }

    match input.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') => {
            let port = port
                .parse()
                .map_err(|_| PingError::InvalidAddress(input.to_string()))?;
            if host.is_empty() {
                return Err(PingError::InvalidAddress(input.to_string()));
            }
            Ok((host.to_string(), port))
        }
        _ => {
            if input.contains(':') {
                return Err(PingError::InvalidAddress(input.to_string()));
            }
            Ok((input.to_string(), DEFAULT_PORT))
        }
    }
}

/// Pings a server and reports its public status. `timeout` covers the whole
/// exchange; callers keep it short because every row pings at once
pub async fn ping_server(address: &str, timeout: Duration) -> Result<ServerStatus, PingError> {
    let (host, port) = parse_address(address)?;

    match tokio::time::timeout(timeout, ping_resolved(&host, port)).await {
        Ok(result) => result,
        Err(_) => Err(PingError::Timeout),
    }
}

async fn ping_resolved(host: &str, port: u16) -> Result<ServerStatus, PingError> {
    let addr = resolve(host, port)?;
    let mut stream = TcpStream::connect(addr).await?;

    let start = Instant::now();

    let mut handshake = Vec::new();
    write_varint(&mut handshake, 0x00);
    write_varint(&mut handshake, -1);
    write_varint(&mut handshake, host.len() as i32);
    handshake.extend_from_slice(host.as_bytes());
    handshake.extend_from_slice(&port.to_be_bytes());
    write_varint(&mut handshake, 1);

    let mut frame = Vec::new();
    write_varint(&mut frame, handshake.len() as i32);
    frame.extend_from_slice(&handshake);
    stream.write_all(&frame).await?;
    stream.write_all(&[0x01, 0x00]).await?;
    stream.flush().await?;

    let length = read_varint(&mut stream).await?;
    if length <= 0 || length > 1024 * 512 {
        return Err(PingError::Protocol);
    }

    let mut payload = vec![0u8; length as usize];
    stream.read_exact(&mut payload).await?;

    let latency_ms = start.elapsed().as_millis() as u64;

    let mut payload_cursor = payload.as_slice();
    if read_varint_from(&mut payload_cursor).await? != 0x00 {
        return Err(PingError::Protocol);
    }
    let json_len = read_varint_from(&mut payload_cursor).await?;
    if json_len <= 0 || json_len as usize > payload_cursor.len() {
        return Err(PingError::Protocol);
    }
    let json = std::str::from_utf8(&payload_cursor[..json_len as usize])
        .map_err(|_| PingError::Protocol)?;

    let response: StatusResponse = serde_json::from_str(json).map_err(|_| PingError::Protocol)?;

    Ok(ServerStatus {
        players_online: response.players.as_ref().map_or(0, |p| p.online),
        players_max: response.players.as_ref().map_or(0, |p| p.max),
        version_name: response
            .version
            .and_then(|v| v.name)
            .unwrap_or_else(|| "unknown".to_string()),
        motd: response
            .description
            .as_ref()
            .map(description_text)
            .unwrap_or_default(),
        latency_ms,
    })
}

fn resolve(host: &str, port: u16) -> Result<SocketAddr, PingError> {
    let lookup = format!("{host}:{port}");
    lookup
        .to_socket_addrs()
        .map_err(|_| PingError::Resolve(host.to_string()))?
        .next()
        .ok_or_else(|| PingError::Resolve(host.to_string()))
}

fn write_varint(out: &mut Vec<u8>, value: i32) {
    let mut value = value as u32;
    loop {
        if value & !0x7F == 0 {
            out.push(value as u8);
            return;
        }
        out.push((value as u8 & 0x7F) | 0x80);
        value >>= 7;
    }
}

async fn read_varint(stream: &mut TcpStream) -> Result<i32, PingError> {
    let mut result = 0u32;
    for shift in 0..5 {
        let mut byte = [0u8; 1];
        let read = stream.read(&mut byte).await?;
        if read == 0 {
            return Err(PingError::Closed);
        }
        let byte = byte[0];
        result |= ((byte & 0x7F) as u32) << (shift * 7);
        if byte & 0x80 == 0 {
            return Ok(result as i32);
        }
    }
    Err(PingError::Protocol)
}

async fn read_varint_from(cursor: &mut &[u8]) -> Result<i32, PingError> {
    let mut result = 0u32;
    for shift in 0..5 {
        let Some((first, rest)) = cursor.split_first() else {
            return Err(PingError::Closed);
        };
        *cursor = rest;
        let byte = *first;
        result |= ((byte & 0x7F) as u32) << (shift * 7);
        if byte & 0x80 == 0 {
            return Ok(result as i32);
        }
    }
    Err(PingError::Protocol)
}

/// `description` is a plain string on some servers and a chat component with
/// nested `extra` entries on others; both flatten to one line of plain text
fn description_text(value: &serde_json::Value) -> String {
    let mut text = String::new();
    collect_text(value, &mut text);
    strip_formatting(&text)
}

fn collect_text(value: &serde_json::Value, out: &mut String) {
    match value {
        serde_json::Value::String(text) => out.push_str(text),
        serde_json::Value::Object(map) => {
            if let Some(serde_json::Value::String(text)) = map.get("text") {
                out.push_str(text);
            }
            if let Some(serde_json::Value::Array(extra)) = map.get("extra") {
                for part in extra {
                    collect_text(part, out);
                }
            }
        }
        serde_json::Value::Array(parts) => {
            for part in parts {
                collect_text(part, out);
            }
        }
        _ => {}
    }
}

/// Legacy `§` colour codes would render as garbage in a plain launcher label
fn strip_formatting(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch == '\u{a7}' {
            let _ = chars.next();
        } else {
            out.push(ch);
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_parse_with_and_without_ports() {
        assert_eq!(
            parse_address("play.example.com").unwrap(),
            ("play.example.com".to_string(), 25565)
        );
        assert_eq!(
            parse_address("play.example.com:25566").unwrap(),
            ("play.example.com".to_string(), 25566)
        );
        assert_eq!(
            parse_address("[::1]:25567").unwrap(),
            ("::1".to_string(), 25567)
        );
        assert_eq!(parse_address("[::1]").unwrap(), ("::1".to_string(), 25565));
        assert!(parse_address("").is_err());
        assert!(parse_address("host:notaport").is_err());
    }

    #[test]
    fn motd_flattens_components_and_drops_colour_codes() {
        let value = serde_json::json!({
            "text": "\u{a7}aHello ",
            "extra": [{ "text": "\u{a7}lWorld" }]
        });
        assert_eq!(description_text(&value), "Hello World");
        assert_eq!(
            description_text(&serde_json::json!("Plain motd")),
            "Plain motd"
        );
    }

    #[test]
    fn varints_round_trip() {
        for value in [0, 1, 127, 128, 255, 300, 25565, i32::MAX, -1] {
            let mut buf = Vec::new();
            write_varint(&mut buf, value);
            let mut cursor = buf.as_slice();
            let decoded = tokio_test_block_on(read_varint_from(&mut cursor)).unwrap();
            assert_eq!(decoded, value, "value {value} should round trip");
        }
    }

    fn tokio_test_block_on<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(future)
    }

    #[tokio::test]
    async fn ping_reads_a_canned_status_from_a_local_listener() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0u8; 256];
            let _ = socket.read(&mut buffer).await;

            let json = r#"{"version":{"name":"1.21.4","protocol":769},"players":{"max":100,"online":42},"description":{"text":"§aHello ","extra":[{"text":"§lWorld"}]}}"#;
            let mut payload = Vec::new();
            write_varint(&mut payload, 0x00);
            write_varint(&mut payload, json.len() as i32);
            payload.extend_from_slice(json.as_bytes());

            let mut frame = Vec::new();
            write_varint(&mut frame, payload.len() as i32);
            frame.extend_from_slice(&payload);
            socket.write_all(&frame).await.unwrap();
        });

        let status = ping_server(&format!("127.0.0.1:{port}"), Duration::from_secs(2))
            .await
            .unwrap();

        assert_eq!(status.players_online, 42);
        assert_eq!(status.players_max, 100);
        assert_eq!(status.version_name, "1.21.4");
        assert_eq!(status.motd, "Hello World");
    }

    #[tokio::test]
    async fn an_unreachable_server_times_out_instead_of_hanging() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let result = ping_server(&format!("127.0.0.1:{port}"), Duration::from_millis(300)).await;
        assert!(matches!(
            result,
            Err(PingError::Timeout) | Err(PingError::Io(_))
        ));
    }
}
