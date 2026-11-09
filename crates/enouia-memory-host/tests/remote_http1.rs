//! ADR-MEM-130 loopback HTTP/1.1 driver; platform-neutral, fake session.
use enouia_memory_contract::ports::SequentialIdSource;
use enouia_memory_host::mcp::remote::{http1, *};
use serde_json::{Value, json};
use std::io::{Cursor, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

struct Echo(Vec<Value>);
impl Session for Echo {
    fn handle(&mut self, message: &Value) -> Option<Value> {
        self.0.push(message.clone());
        message
            .get("id")
            .map(|id| json!({"jsonrpc":"2.0","id":id,"result":{"echo":message["method"]}}))
    }
    fn closed(&self) -> bool {
        false
    }
    fn unknown(&self) -> Option<&Value> {
        None
    }
}
struct Fixed;
impl Tokens for Fixed {
    fn verify(&self, token: &str) -> Option<Bearer> {
        (token == "tok-owner").then(|| Bearer {
            subject: "owner".into(),
            scopes: vec![READ_SCOPE.into()],
        })
    }
}
fn config() -> Config {
    Config::new("https://memory.example/mcp", &["https://auth.example"], &[]).unwrap()
}
struct Duplex {
    input: Cursor<Vec<u8>>,
    output: Vec<u8>,
}
impl Read for Duplex {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.input.read(buf)
    }
}
impl Write for Duplex {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.output.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn raw_post(body: &str, session: Option<&str>) -> Vec<u8> {
    let mut head = format!(
        "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nAccept: application/json, text/event-stream\r\nAuthorization: Bearer tok-owner\r\nContent-Length: {}\r\n",
        body.len()
    );
    if let Some(session) = session {
        head.push_str(&format!(
            "Mcp-Session-Id: {session}\r\nMCP-Protocol-Version: 2025-11-25\r\n"
        ));
    }
    format!("{head}\r\n{body}").into_bytes()
}
const INIT: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"synthetic","version":"1"}}}"#;

fn status_and_head(output: &[u8]) -> (u16, String, Vec<u8>) {
    let text = String::from_utf8_lossy(output).to_string();
    let end = text.find("\r\n\r\n").unwrap();
    let status = text[9..12].parse().unwrap();
    (status, text[..end].to_owned(), output[end + 4..].to_vec())
}
fn once(endpoint: &mut Endpoint<'_, Echo>, bytes: Vec<u8>) -> (u16, String, Vec<u8>) {
    let mut duplex = Duplex {
        input: Cursor::new(bytes),
        output: vec![],
    };
    http1::exchange(&mut duplex, endpoint).unwrap();
    status_and_head(&duplex.output)
}

#[test]
fn non_loopback_bind_is_refused() {
    for addr in ["0.0.0.0:0", "192.0.2.1:8080", "[::]:0"] {
        let error = http1::bind_loopback(addr.parse::<SocketAddr>().unwrap()).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    }
}

#[test]
fn well_formed_requests_reach_the_endpoint_and_answer_with_length_and_close() {
    let ids = SequentialIdSource::new(5);
    let mut endpoint = Endpoint::new(config(), &Fixed, &ids, |_: &Bearer| Some(Echo(vec![])));
    let (status, head, body) = once(&mut endpoint, raw_post(INIT, None));
    assert_eq!(status, 200);
    assert!(head.starts_with("HTTP/1.1 200 OK\r\n"));
    assert!(head.contains("Mcp-Session-Id: 00000000000000000000000000000005"));
    assert!(head.contains("Cache-Control: no-store"));
    assert!(head.contains(&format!("Content-Length: {}", body.len())));
    assert!(head.ends_with("Connection: close"));
    assert_eq!(serde_json::from_slice::<Value>(&body).unwrap()["id"], 1);
    let note = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
    let session = "00000000000000000000000000000005";
    let (status, head, body) = once(&mut endpoint, raw_post(note, Some(session)));
    assert_eq!((status, body.is_empty()), (202, true));
    assert!(head.contains("Content-Length: 0"));
    let (status, _, _) = once(
        &mut endpoint,
        b"GET /.well-known/oauth-protected-resource/mcp HTTP/1.1\r\nHost: x\r\n\r\n".to_vec(),
    );
    assert_eq!(status, 200);
    let mut pipelined = raw_post(
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        Some(session),
    );
    pipelined.extend_from_slice(&raw_post(
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call"}"#,
        Some(session),
    ));
    let (status, _, body) = once(&mut endpoint, pipelined);
    assert_eq!(
        (
            status,
            serde_json::from_slice::<Value>(&body).unwrap()["id"].clone()
        ),
        (200, json!(2))
    );
    assert_eq!(endpoint.sessions(), 1);
}

#[test]
fn malformed_framing_is_refused_before_the_endpoint() {
    let ids = SequentialIdSource::new(1);
    let mut opened = 0;
    let mut endpoint = Endpoint::new(config(), &Fixed, &ids, |_: &Bearer| {
        opened += 1;
        Some(Echo(vec![]))
    });
    let post = String::from_utf8(raw_post(INIT, None)).unwrap();
    let cases: Vec<(Vec<u8>, u16)> = vec![
        (post.replacen("HTTP/1.1", "HTTP/1.0", 1).into_bytes(), 505),
        (
            post.replacen("POST /mcp", "POST  /mcp", 1).into_bytes(),
            400,
        ),
        (
            post.replacen("POST /mcp", "POST https://memory.example/mcp", 1)
                .into_bytes(),
            400,
        ),
        (
            post.replacen("Host: 127.0.0.1\r\n", "", 1).into_bytes(),
            400,
        ),
        (
            post.replacen("Host: 127.0.0.1\r\n", "Host: a\r\nHost: b\r\n", 1)
                .into_bytes(),
            400,
        ),
        (
            post.replacen(
                "Content-Length",
                "Transfer-Encoding: chunked\r\nContent-Length",
                1,
            )
            .into_bytes(),
            501,
        ),
        (
            post.replacen(&format!("Content-Length: {}\r\n", INIT.len()), "", 1)
                .into_bytes(),
            411,
        ),
        (
            post.replacen(
                &format!("Content-Length: {}", INIT.len()),
                "Content-Length: +5",
                1,
            )
            .into_bytes(),
            400,
        ),
        (
            post.replacen(
                &format!("Content-Length: {}", INIT.len()),
                &format!(
                    "Content-Length: {}\r\nContent-Length: {}",
                    INIT.len(),
                    INIT.len()
                ),
                1,
            )
            .into_bytes(),
            400,
        ),
        (
            post.replacen(
                &format!("Content-Length: {}", INIT.len()),
                "Content-Length: 262145",
                1,
            )
            .into_bytes(),
            413,
        ),
        (
            post.replacen(
                &format!("Content-Length: {}", INIT.len()),
                "Content-Length: 99999999999999999999999",
                1,
            )
            .into_bytes(),
            413,
        ),
        (post.replacen("Accept:", " Accept:", 1).into_bytes(), 400),
        (
            post.replacen(
                "Accept: application/json",
                "Accept: application/json\u{1}",
                1,
            )
            .into_bytes(),
            400,
        ),
        (post.replacen("Accept:", "Bad Name:", 1).into_bytes(), 400),
        (
            post.replacen(
                &format!("Content-Length: {}", INIT.len()),
                &format!("Content-Length: {}", INIT.len() + 10),
                1,
            )
            .into_bytes(),
            400,
        ),
        (
            format!(
                "GET /mcp HTTP/1.1\r\nHost: x\r\nX: {}\r\n\r\n",
                "a".repeat(17 * 1024)
            )
            .into_bytes(),
            431,
        ),
        (
            ((0..70).fold("GET /mcp HTTP/1.1\r\nHost: x\r\n".to_owned(), |s, n| {
                s + &format!("X-{n}: v\r\n")
            }) + "\r\n")
                .into_bytes(),
            431,
        ),
        (b"POST /mcp HTTP/1.1\r\nHost: x\r\n".to_vec(), 400),
    ];
    for (n, (bytes, want)) in cases.into_iter().enumerate() {
        let (status, head, body) = once(&mut endpoint, bytes);
        assert_eq!(status, want, "case {n}: {head}");
        assert!(
            body.is_empty() && head.ends_with("Connection: close"),
            "case {n}"
        );
    }
    let mut empty = Duplex {
        input: Cursor::new(vec![]),
        output: vec![],
    };
    assert!(http1::exchange(&mut empty, &mut endpoint).is_err());
    assert!(empty.output.is_empty());
    drop(endpoint);
    assert_eq!(opened, 0);
}

#[test]
fn real_loopback_sockets_serve_sequential_connections_and_stop_on_request() {
    let listener = http1::bind_loopback("127.0.0.1:0".parse().unwrap()).unwrap();
    let addr = listener.local_addr().unwrap();
    let client = std::thread::spawn(move || {
        let send = |bytes: &[u8]| {
            let mut stream = TcpStream::connect(addr).unwrap();
            stream.write_all(bytes).unwrap();
            let mut out = vec![];
            stream.read_to_end(&mut out).unwrap();
            out
        };
        let opened = send(&raw_post(INIT, None));
        let (_, head, _) = status_and_head(&opened);
        let session = head
            .lines()
            .find_map(|l| l.strip_prefix("Mcp-Session-Id: "))
            .unwrap()
            .to_owned();
        // A client that connects and sends nothing only costs one timeout.
        let idle = TcpStream::connect(addr).unwrap();
        let listed = send(&raw_post(
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
            Some(&session),
        ));
        drop(idle);
        let unauthorized = send(
            &String::from_utf8(raw_post(INIT, None))
                .unwrap()
                .replace("tok-owner", "tok-forged")
                .into_bytes(),
        );
        (opened, listed, unauthorized)
    });
    let ids = SequentialIdSource::new(9);
    let mut endpoint = Endpoint::new(config(), &Fixed, &ids, |_: &Bearer| Some(Echo(vec![])));
    http1::serve(
        &listener,
        &mut endpoint,
        Duration::from_millis(300),
        |_, served| served < 4,
    )
    .unwrap();
    let (opened, listed, unauthorized) = client.join().unwrap();
    assert_eq!(status_and_head(&opened).0, 200);
    let (status, _, body) = status_and_head(&listed);
    assert_eq!(
        (status, serde_json::from_slice::<Value>(&body).unwrap()),
        (
            200,
            json!({"jsonrpc":"2.0","id":2,"result":{"echo":"tools/list"}})
        )
    );
    let (status, head, _) = status_and_head(&unauthorized);
    assert_eq!(status, 401);
    assert!(head.contains("WWW-Authenticate: Bearer resource_metadata="));
    assert_eq!(endpoint.sessions(), 1);
}
