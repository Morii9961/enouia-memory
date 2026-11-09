//! Streamable HTTP endpoint for remote MCP clients such as a ChatGPT
//! developer-mode connector (ADR-MEM-129). Transport-neutral: the caller's
//! HTTPS listener hands over one parsed request and writes back the response.
//! No listener, TLS, authorization server or deployment lives here.
use super::{MAX_INPUT_BYTES, PROTOCOL_VERSION, json as unique};

pub mod http1;
pub mod oauth;
pub mod service;
use enouia_memory_contract::ports::IdSource;
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub const READ_SCOPE: &str = "memory.read";
pub const MAX_SESSIONS: usize = 4;
const METADATA_PATH: &str = "/.well-known/oauth-protected-resource";

/// One MCP server instance bound to one remote session.
pub trait Session {
    fn handle(&mut self, message: &Value) -> Option<Value>;
    fn closed(&self) -> bool;
    /// The complete first message whose outcome is unknown, if any.
    fn unknown(&self) -> Option<&Value>;
}

impl Session for super::opening::Server<'_, '_> {
    fn handle(&mut self, message: &Value) -> Option<Value> {
        super::opening::Server::handle(self, message)
    }
    fn closed(&self) -> bool {
        super::opening::Server::closed(self)
    }
    fn unknown(&self) -> Option<&Value> {
        self.original_unknown()
    }
}

/// A bearer token the authorization side already verified for this resource
/// (issuer, signature, expiry and audience are the verifier's job).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bearer {
    pub subject: String,
    pub scopes: Vec<String>,
}

pub trait Tokens {
    fn verify(&self, token: &str) -> Option<Bearer>;
}

pub struct Config {
    resource: String,
    path: String,
    metadata: String,
    authorization_servers: Vec<String>,
    origins: Vec<String>,
}

impl Config {
    /// `resource` is the public `https://host/path` MCP URL. Plain HTTP and
    /// URLs with query, fragment or user info are refused.
    pub fn new(resource: &str, authorization_servers: &[&str], origins: &[&str]) -> Option<Self> {
        fn https(url: &str) -> Option<&str> {
            url.strip_prefix("https://").filter(|rest| {
                !rest.is_empty() && !rest.contains(['?', '#', '@', ' ']) && !rest.starts_with('/')
            })
        }
        let rest = https(resource)?;
        let (host, path) = rest.split_at(rest.find('/')?);
        if path.len() < 2 || path.ends_with('/') || authorization_servers.is_empty() {
            return None;
        }
        if !authorization_servers.iter().all(|s| https(s).is_some()) {
            return None;
        }
        Some(Self {
            resource: resource.to_owned(),
            path: path.to_owned(),
            metadata: format!("https://{host}{METADATA_PATH}{path}"),
            authorization_servers: authorization_servers
                .iter()
                .map(|s| s.to_string())
                .collect(),
            origins: origins.iter().map(|s| s.to_string()).collect(),
        })
    }
}

pub struct Request {
    pub method: String,
    /// Request target as received, e.g. `/mcp`.
    pub target: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    fn new(status: u16) -> Self {
        Self {
            status,
            headers: vec![("Cache-Control".into(), "no-store".into())],
            body: vec![],
        }
    }
    fn json(status: u16, value: &Value) -> Self {
        let mut response = Self::new(status);
        response
            .headers
            .push(("Content-Type".into(), "application/json".into()));
        response.body = value.to_string().into_bytes();
        response
    }
    fn header(mut self, name: &str, value: String) -> Self {
        self.headers.push((name.into(), value));
        self
    }
    pub fn header_value(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// Builds the session for a verified bearer; `None` refuses it.
pub type Open<'f, S> = Box<dyn FnMut(&Bearer) -> Option<S> + 'f>;

pub struct Endpoint<'f, S: Session> {
    config: Config,
    tokens: &'f dyn Tokens,
    ids: &'f dyn IdSource,
    open: Open<'f, S>,
    sessions: BTreeMap<String, (String, S)>,
    halted: Option<Value>,
}

/// Anything the HTTP driver can serve.
pub trait Handler {
    fn respond(&mut self, request: &Request) -> Response;
}

impl<S: Session> Handler for Endpoint<'_, S> {
    fn respond(&mut self, request: &Request) -> Response {
        Endpoint::respond(self, request)
    }
}

/// The OAuth authority's paths first, then the MCP endpoint, which verifies
/// bearer tokens against that same authority.
pub struct Router<'f, S: Session> {
    pub authority: &'f oauth::Authority<'f>,
    pub endpoint: Endpoint<'f, S>,
}

impl<S: Session> Handler for Router<'_, S> {
    fn respond(&mut self, request: &Request) -> Response {
        match self.authority.respond(request) {
            Some(response) => response,
            None => self.endpoint.respond(request),
        }
    }
}

impl<'f, S: Session> Endpoint<'f, S> {
    pub fn new(
        config: Config,
        tokens: &'f dyn Tokens,
        ids: &'f dyn IdSource,
        open: impl FnMut(&Bearer) -> Option<S> + 'f,
    ) -> Self {
        Self {
            config,
            tokens,
            ids,
            open: Box::new(open),
            sessions: BTreeMap::new(),
            halted: None,
        }
    }
    pub fn sessions(&self) -> usize {
        self.sessions.len()
    }
    /// The first message whose outcome became unknown. The endpoint then
    /// serves nothing until the operator recovers and builds a new one.
    pub fn halted(&self) -> Option<&Value> {
        self.halted.as_ref()
    }

    pub fn respond(&mut self, request: &Request) -> Response {
        let mut headers: BTreeMap<String, Vec<&str>> = BTreeMap::new();
        for (name, value) in &request.headers {
            headers
                .entry(name.to_ascii_lowercase())
                .or_default()
                .push(value.trim());
        }
        let one = |name: &str| match headers.get(name).map(Vec::as_slice) {
            None => Ok(None),
            Some([value]) => Ok(Some(*value)),
            Some(_) => Err(()),
        };
        let metadata = format!("{METADATA_PATH}{}", self.config.path);
        if request.method == "GET"
            && (request.target == metadata || request.target == METADATA_PATH)
        {
            return Response::json(
                200,
                &json!({
                    "resource": self.config.resource,
                    "authorization_servers": self.config.authorization_servers,
                    "scopes_supported": [READ_SCOPE],
                    "bearer_methods_supported": ["header"],
                }),
            );
        }
        if request.target != self.config.path {
            return Response::new(404);
        }
        if self.halted.is_some() {
            return Response::new(503);
        }
        match one("origin") {
            Ok(None) => {}
            Ok(Some(origin)) if self.config.origins.iter().any(|o| o == origin) => {}
            _ => return Response::new(403),
        }
        let challenge = |error: Option<&str>| {
            let mut value = format!("Bearer resource_metadata=\"{}\"", self.config.metadata);
            if let Some(error) = error {
                value.push_str(&format!(", error=\"{error}\""));
            }
            value
        };
        let token = match one("authorization") {
            Ok(Some(value)) => value
                .split_once(' ')
                .filter(|(scheme, token)| {
                    scheme.eq_ignore_ascii_case("bearer") && !token.is_empty()
                })
                .map(|(_, token)| token.trim()),
            Ok(None) => return Response::new(401).header("WWW-Authenticate", challenge(None)),
            Err(()) => return Response::new(400),
        };
        let Some(bearer) = token.and_then(|token| self.tokens.verify(token)) else {
            return Response::new(401).header("WWW-Authenticate", challenge(Some("invalid_token")));
        };
        if !bearer.scopes.iter().any(|s| s == READ_SCOPE) {
            return Response::new(403).header(
                "WWW-Authenticate",
                challenge(Some("insufficient_scope")) + &format!(", scope=\"{READ_SCOPE}\""),
            );
        }
        let Ok(session_id) = one("mcp-session-id") else {
            return Response::new(400);
        };
        match request.method.as_str() {
            "GET" => return Response::new(405).header("Allow", "POST, DELETE".into()),
            "DELETE" => {
                let Some(id) = session_id else {
                    return Response::new(400);
                };
                return match self.sessions.get(id) {
                    Some((subject, _)) if *subject == bearer.subject => {
                        self.sessions.remove(id);
                        Response::new(204)
                    }
                    _ => Response::new(404),
                };
            }
            "POST" => {}
            _ => return Response::new(405).header("Allow", "POST, DELETE".into()),
        }
        let media = |name: &str, wanted: &[&str]| {
            one(name).ok().flatten().is_some_and(|value| {
                value.split(',').any(|part| {
                    let kind = part.split(';').next().unwrap_or("").trim();
                    wanted.iter().any(|w| kind.eq_ignore_ascii_case(w))
                })
            })
        };
        if !media("content-type", &["application/json"]) {
            return Response::new(415);
        }
        if !media("accept", &["application/json", "*/*"]) {
            return Response::new(406);
        }
        if request.body.len() > MAX_INPUT_BYTES {
            return Response::new(413);
        }
        let Ok(message) = unique::parse(&request.body) else {
            return Response::json(
                400,
                &json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"parse_error"}}),
            );
        };
        if !message.is_object() {
            // JSON-RPC batches are not part of this protocol version.
            return Response::json(
                400,
                &json!({"jsonrpc":"2.0","id":null,"error":{"code":-32600,"message":"invalid_request"}}),
            );
        }
        if message["method"] == "initialize" && message.get("id").is_some() {
            if session_id.is_some() {
                return Response::new(400);
            }
            if self.sessions.len() >= MAX_SESSIONS {
                return Response::new(503);
            }
            let Some(mut session) = (self.open)(&bearer) else {
                return Response::new(403);
            };
            let response = session.handle(&message);
            if let Some(original) = session.unknown() {
                return self.halt(original.clone());
            }
            let Some(response) = response else {
                return Response::new(400);
            };
            if response.get("result").is_none() || session.closed() {
                return Response::json(200, &response);
            }
            let id: String = self
                .ids
                .random_16()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            self.sessions.insert(id.clone(), (bearer.subject, session));
            return Response::json(200, &response).header("Mcp-Session-Id", id);
        }
        let Some(id) = session_id else {
            return Response::new(400);
        };
        if one("mcp-protocol-version") != Ok(Some(PROTOCOL_VERSION)) {
            return Response::new(400);
        }
        let Some((subject, session)) = self.sessions.get_mut(id) else {
            return Response::new(404);
        };
        if *subject != bearer.subject {
            return Response::new(404);
        }
        let response = session.handle(&message);
        if let Some(original) = session.unknown() {
            let original = original.clone();
            return self.halt(original);
        }
        if session.closed() {
            self.sessions.remove(id);
        }
        match response {
            Some(response) => Response::json(200, &response),
            None => Response::new(202),
        }
    }

    fn halt(&mut self, original: Value) -> Response {
        self.sessions.clear();
        self.halted = Some(original);
        Response::new(503)
    }
}
