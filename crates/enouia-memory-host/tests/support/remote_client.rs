#![allow(dead_code)]
//! A ChatGPT-style client for the remote service over real loopback sockets.
use enouia_memory_contract::hash::sha256;
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};

pub const REDIRECT: &str = "https://chatgpt.com/connector_platform_oauth_redirect";
pub const VERIFIER: &str = "synthetic-verifier-0123456789-abcdefghijklmnopqrstuvwxyz";

pub fn challenge(verifier: &str) -> String {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let hex = sha256(verifier.as_bytes());
    let raw: Vec<u8> = (0..32)
        .map(|i| u8::from_str_radix(&hex.as_str()[2 * i..2 * i + 2], 16).unwrap())
        .collect();
    let mut out = String::new();
    for chunk in raw.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | (*b as u32) << (16 - 8 * i));
        for i in 0..=chunk.len() {
            out.push(A[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    out
}
pub fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
pub fn form(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{k}={}", enc(v)))
        .collect::<Vec<_>>()
        .join("&")
}
pub struct Reply {
    pub status: u16,
    pub head: String,
    pub body: Vec<u8>,
}
impl Reply {
    pub fn header(&self, name: &str) -> Option<String> {
        self.head.lines().find_map(|l| {
            let (n, v) = l.split_once(": ")?;
            n.eq_ignore_ascii_case(name).then(|| v.to_owned())
        })
    }
    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap()
    }
}
pub fn send(
    addr: SocketAddr,
    method: &str,
    target: &str,
    headers: &[(&str, String)],
    body: &str,
) -> Reply {
    let mut stream = TcpStream::connect(addr).unwrap();
    let mut raw = format!(
        "{method} {target} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\n",
        body.len()
    );
    for (n, v) in headers {
        raw.push_str(&format!("{n}: {v}\r\n"));
    }
    raw.push_str("\r\n");
    raw.push_str(body);
    stream.write_all(raw.as_bytes()).unwrap();
    let mut out = vec![];
    stream.read_to_end(&mut out).unwrap();
    let end = out.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    let head = String::from_utf8(out[..end].to_vec()).unwrap();
    Reply {
        status: head[9..12].parse().unwrap(),
        head,
        body: out[end + 4..].to_vec(),
    }
}
pub fn mcp(addr: SocketAddr, token: &str, session: Option<&str>, body: Value) -> Reply {
    let mut headers = vec![
        ("Content-Type", "application/json".to_owned()),
        ("Accept", "application/json, text/event-stream".to_owned()),
        ("Authorization", format!("Bearer {token}")),
    ];
    if let Some(session) = session {
        headers.push(("Mcp-Session-Id", session.to_owned()));
        headers.push(("MCP-Protocol-Version", "2025-11-25".to_owned()));
    }
    send(addr, "POST", "/mcp", &headers, &body.to_string())
}
pub fn initialize(addr: SocketAddr, token: &str) -> String {
    let opened = mcp(
        addr,
        token,
        None,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"synthetic-remote","version":"1"}}}),
    );
    assert_eq!(
        opened.status,
        200,
        "{}",
        String::from_utf8_lossy(&opened.body)
    );
    let session = opened.header("mcp-session-id").unwrap();
    assert_eq!(
        mcp(
            addr,
            token,
            Some(&session),
            json!({"jsonrpc":"2.0","method":"notifications/initialized"})
        )
        .status,
        202
    );
    session
}
