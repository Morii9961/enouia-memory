//! Loopback HTTP/1.1 driver for [`super::Endpoint`] or [`super::Router`] (ADR-MEM-130). It listens only
//! on a loopback address behind a TLS-terminating reverse proxy, answers one
//! request per connection and closes it. Deliberately narrow: HTTP/1.1 only,
//! `Content-Length` or chunked request bodies (a tunnel such as cloudflared may
//! forward chunked bodies without buffering), no pipelining or upgrades.
use super::{Handler, Request, Response};
use crate::mcp::MAX_INPUT_BYTES;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::time::Duration;

pub const MAX_HEAD_BYTES: usize = 16 * 1024;
const MAX_HEADERS: usize = 64;

/// Bind the driver's listener. Anything but a loopback address is refused:
/// public traffic reaches it only through the TLS proxy on the same machine.
pub fn bind_loopback(addr: SocketAddr) -> io::Result<TcpListener> {
    if !addr.ip().is_loopback() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "remote MCP listens on loopback only",
        ));
    }
    TcpListener::bind(addr)
}

/// Serve connections one at a time until `keep_going(handler, served)` says
/// stop. It is asked before every accept and again every 50 ms while idle, so
/// time-based stops happen without waiting for traffic. A failed connection
/// never stops the loop; a failed `accept` does.
pub fn serve<H: Handler>(
    listener: &TcpListener,
    endpoint: &mut H,
    timeout: Duration,
    mut keep_going: impl FnMut(&H, usize) -> bool,
) -> io::Result<()> {
    listener.set_nonblocking(true)?;
    let mut served = 0;
    while keep_going(endpoint, served) {
        match listener.accept() {
            Ok((mut stream, _)) => {
                // Windows sockets inherit the listener's non-blocking mode.
                if stream.set_nonblocking(false).is_ok()
                    && stream.set_read_timeout(Some(timeout)).is_ok()
                    && stream.set_write_timeout(Some(timeout)).is_ok()
                {
                    let _ = exchange(&mut stream, endpoint);
                }
                served += 1;
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

/// Read one request, answer it and leave the stream for the caller to drop.
pub fn exchange(stream: &mut (impl Read + Write), endpoint: &mut impl Handler) -> io::Result<()> {
    let response = match read_request(stream) {
        Ok(request) => endpoint.respond(&request),
        Err(Refusal::Status(status)) => Response::new(status),
        Err(Refusal::Io(error)) => return Err(error),
    };
    let mut head = format!(
        "HTTP/1.1 {} {}\r\n",
        response.status,
        reason(response.status)
    );
    for (name, value) in &response.headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str(&format!(
        "Content-Length: {}\r\nConnection: close\r\n\r\n",
        response.body.len()
    ));
    stream.write_all(head.as_bytes())?;
    stream.write_all(&response.body)?;
    stream.flush()
}

enum Refusal {
    Status(u16),
    Io(io::Error),
}

fn read_request(stream: &mut (impl Read + Write)) -> Result<Request, Refusal> {
    let mut buffer = Vec::with_capacity(1024);
    let mut chunk = [0u8; 4096];
    let end = loop {
        if let Some(n) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            break n;
        }
        if buffer.len() > MAX_HEAD_BYTES {
            return Err(Refusal::Status(431));
        }
        match stream.read(&mut chunk) {
            Ok(0) if buffer.is_empty() => {
                return Err(Refusal::Io(io::ErrorKind::UnexpectedEof.into()));
            }
            Ok(0) => return Err(Refusal::Status(400)),
            Ok(n) => buffer.extend_from_slice(&chunk[..n]),
            Err(error) => return Err(Refusal::Io(error)),
        }
    };
    if end > MAX_HEAD_BYTES {
        return Err(Refusal::Status(431));
    }
    let bad = Refusal::Status(400);
    let head = std::str::from_utf8(&buffer[..end]).map_err(|_| Refusal::Status(400))?;
    let mut lines = head.split("\r\n");
    let mut start = lines.next().unwrap_or("").split(' ');
    let (Some(method), Some(target), Some(version), None) =
        (start.next(), start.next(), start.next(), start.next())
    else {
        return Err(bad);
    };
    let token = |s: &str| {
        !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
    };
    if !token(method) || !target.starts_with('/') || !target.bytes().all(|b| b.is_ascii_graphic()) {
        return Err(bad);
    }
    if version != "HTTP/1.1" {
        return Err(Refusal::Status(505));
    }
    let mut headers = Vec::new();
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            return Err(bad);
        };
        if !token(name) || value.bytes().any(|b| b.is_ascii_control() && b != b'\t') {
            return Err(bad);
        }
        headers.push((name.to_owned(), value.trim_matches([' ', '\t']).to_owned()));
        if headers.len() > MAX_HEADERS {
            return Err(Refusal::Status(431));
        }
    }
    let named = |wanted: &str| -> Vec<&str> {
        headers
            .iter()
            .filter(|(n, _)| n.eq_ignore_ascii_case(wanted))
            .map(|(_, v)| v.as_str())
            .collect()
    };
    if named("host").len() != 1 {
        return Err(bad);
    }
    let chunked = match named("transfer-encoding").as_slice() {
        [] => false,
        [coding] if coding.eq_ignore_ascii_case("chunked") => true,
        _ => return Err(Refusal::Status(501)),
    };
    // ADR-MEM-201: a client or proxy waiting on `Expect: 100-continue` gets
    // the interim response once the head is accepted, instead of a stall.
    let expects = named("expect")
        .iter()
        .any(|v| v.eq_ignore_ascii_case("100-continue"));
    if chunked {
        // Both framings at once is ambiguous (RFC 9112 6.1): refuse it.
        if !named("content-length").is_empty() {
            return Err(Refusal::Status(400));
        }
        if expects {
            stream
                .write_all(b"HTTP/1.1 100 Continue\r\n\r\n")
                .map_err(Refusal::Io)?;
        }
        let mut input = io::Cursor::new(buffer[end + 4..].to_vec()).chain(stream);
        let body = read_chunked(&mut input)?;
        return Ok(Request {
            method: method.to_owned(),
            target: target.to_owned(),
            headers,
            body,
        });
    }
    let length = match named("content-length").as_slice() {
        [] if method == "POST" => return Err(Refusal::Status(411)),
        [] => 0,
        [value] if !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) => {
            value.parse::<usize>().map_err(|_| Refusal::Status(413))?
        }
        _ => return Err(bad),
    };
    if length > MAX_INPUT_BYTES {
        return Err(Refusal::Status(413));
    }
    if expects && length > 0 {
        stream
            .write_all(b"HTTP/1.1 100 Continue\r\n\r\n")
            .map_err(Refusal::Io)?;
    }
    let mut body = buffer[end + 4..].to_vec();
    if body.len() > length {
        // Pipelined bytes are never read as another request.
        body.truncate(length);
    }
    let missing = length - body.len();
    let mut rest = vec![0u8; missing];
    stream
        .read_exact(&mut rest)
        .map_err(|error| match error.kind() {
            io::ErrorKind::UnexpectedEof => Refusal::Status(400),
            _ => Refusal::Io(error),
        })?;
    body.extend_from_slice(&rest);
    Ok(Request {
        method: method.to_owned(),
        target: target.to_owned(),
        headers,
        body,
    })
}

/// Decodes a chunked body, bounded by [`MAX_INPUT_BYTES`]. Chunk extensions
/// and trailers are read and discarded; bytes after the body are never read.
fn read_chunked(input: &mut impl Read) -> Result<Vec<u8>, Refusal> {
    fn eof(error: io::Error) -> Refusal {
        match error.kind() {
            io::ErrorKind::UnexpectedEof => Refusal::Status(400),
            _ => Refusal::Io(error),
        }
    }
    fn line(input: &mut impl Read) -> Result<String, Refusal> {
        let mut out = Vec::new();
        loop {
            let mut byte = [0u8];
            input.read_exact(&mut byte).map_err(eof)?;
            if byte[0] == b'\n' {
                break;
            }
            out.push(byte[0]);
            if out.len() > MAX_HEAD_BYTES {
                return Err(Refusal::Status(431));
            }
        }
        if out.pop() != Some(b'\r') {
            return Err(Refusal::Status(400));
        }
        String::from_utf8(out).map_err(|_| Refusal::Status(400))
    }
    let mut body = Vec::new();
    loop {
        let size_line = line(input)?;
        let size = size_line
            .split(';')
            .next()
            .unwrap_or("")
            .trim_matches([' ', '\t']);
        if size.is_empty() || size.len() > 16 || !size.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(Refusal::Status(400));
        }
        let size = u64::from_str_radix(size, 16).map_err(|_| Refusal::Status(400))?;
        if size == 0 {
            break;
        }
        if body.len() as u64 + size > MAX_INPUT_BYTES as u64 {
            return Err(Refusal::Status(413));
        }
        let start = body.len();
        body.resize(start + size as usize, 0);
        input.read_exact(&mut body[start..]).map_err(eof)?;
        if !line(input)?.is_empty() {
            return Err(Refusal::Status(400));
        }
    }
    let mut trailers = 0;
    while !line(input)?.is_empty() {
        trailers += 1;
        if trailers > MAX_HEADERS {
            return Err(Refusal::Status(431));
        }
    }
    Ok(body)
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        202 => "Accepted",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        406 => "Not Acceptable",
        411 => "Length Required",
        413 => "Content Too Large",
        415 => "Unsupported Media Type",
        431 => "Request Header Fields Too Large",
        501 => "Not Implemented",
        503 => "Service Unavailable",
        505 => "HTTP Version Not Supported",
        _ => "",
    }
}
