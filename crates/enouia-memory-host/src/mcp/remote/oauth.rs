//! Single-owner OAuth 2.1 authorization server for the remote endpoint
//! (ADR-MEM-132). Public clients register dynamically (RFC 7591) and use the
//! authorization code grant with S256 PKCE. The owner approves a client by
//! typing a short-lived one-time approval code obtained out of band (on the
//! server over SSH), so no password is stored. Only SHA-256 digests of codes
//! and tokens are kept; refresh tokens rotate, and replaying a used code or
//! refresh token revokes its whole grant family.
use super::{Bearer, READ_SCOPE, Request, Response, Tokens};
use enouia_memory_contract::{foundation::Clock, hash::sha256, ports::IdSource};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::cell::RefCell;
use std::collections::BTreeMap;

pub const ACCESS_TTL_MS: i64 = 60 * 60 * 1000;
pub const REFRESH_TTL_MS: i64 = 30 * 24 * 60 * 60 * 1000;
pub const CODE_TTL_MS: i64 = 2 * 60 * 1000;
pub const APPROVAL_TTL_MS: i64 = 10 * 60 * 1000;
pub const MAX_APPROVAL_FAILURES: u32 = 5;
pub const MAX_CLIENTS: usize = 16;
const METADATA: &str = "/.well-known/oauth-authorization-server";
const REGISTER: &str = "/oauth/register";
const AUTHORIZE: &str = "/oauth/authorize";
const TOKEN: &str = "/oauth/token";
/// Crockford base32 without I, L, O, U: easy to read aloud and type.
const APPROVAL_ALPHABET: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

pub struct OAuthConfig {
    issuer: String,
    resource: String,
    redirect_prefixes: Vec<String>,
}

impl OAuthConfig {
    /// `issuer` is the server origin (`https://host`), `resource` the MCP URL
    /// on that origin, and every redirect prefix an `https://host/` prefix
    /// (for ChatGPT, `https://chatgpt.com/`).
    pub fn new(issuer: &str, resource: &str, redirect_prefixes: &[&str]) -> Option<Self> {
        let origin = |url: &str| {
            url.strip_prefix("https://")
                .is_some_and(|rest| !rest.is_empty() && !rest.contains(['/', '?', '#', '@', ' ']))
        };
        let prefix_ok = |p: &&str| {
            p.strip_prefix("https://")
                .and_then(|rest| rest.split_once('/'))
                .is_some_and(|(host, _)| !host.is_empty() && !host.contains(['@', ' ', '?', '#']))
        };
        let under_issuer = resource
            .strip_prefix(issuer)
            .is_some_and(|path| path.starts_with('/') && path.len() > 1);
        if !origin(issuer)
            || !under_issuer
            || redirect_prefixes.is_empty()
            || !redirect_prefixes.iter().all(prefix_ok)
        {
            return None;
        }
        Some(Self {
            issuer: issuer.to_owned(),
            resource: resource.to_owned(),
            redirect_prefixes: redirect_prefixes.iter().map(|p| p.to_string()).collect(),
        })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct State {
    clients: BTreeMap<String, Vec<String>>,
    approval: Option<Approval>,
    codes: BTreeMap<String, Grant>,
    access: BTreeMap<String, Issued>,
    refresh: BTreeMap<String, Issued>,
    /// Digests of consumed codes and rotated refresh tokens, kept until they
    /// would have expired so a replay can revoke the family.
    spent: BTreeMap<String, Issued>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Approval {
    digest: String,
    expires_ms: i64,
    failures: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Grant {
    client_id: String,
    redirect_uri: String,
    challenge: String,
    family: String,
    expires_ms: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Issued {
    client_id: String,
    family: String,
    expires_ms: i64,
}

pub struct Authority<'a> {
    config: OAuthConfig,
    ids: &'a dyn IdSource,
    clock: &'a dyn Clock,
    state: RefCell<State>,
}

fn digest(secret: &str) -> String {
    sha256(secret.as_bytes()).as_str().to_owned()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn base64url(bytes: &[u8]) -> String {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
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

fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' => {
                let pair = s.get(i + 1..i + 3)?;
                out.push(u8::from_str_radix(pair, 16).ok()?);
                i += 2;
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8(out).ok()
}

fn percent_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// `application/x-www-form-urlencoded`; duplicate or undecodable keys refuse.
fn form(s: &str) -> Option<BTreeMap<String, String>> {
    let mut map = BTreeMap::new();
    for pair in s.split('&').filter(|p| !p.is_empty()) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        if map.insert(percent_decode(k)?, percent_decode(v)?).is_some() {
            return None;
        }
    }
    Some(map)
}

fn html_escape(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '&' => "&amp;".into(),
            '<' => "&lt;".into(),
            '>' => "&gt;".into(),
            '"' => "&quot;".into(),
            '\'' => "&#39;".into(),
            c => c.to_string(),
        })
        .collect()
}

fn oauth_error(status: u16, error: &str) -> Response {
    Response::json(status, &json!({"error": error}))
}

fn page(status: u16, body: &str) -> Response {
    let mut response = Response::new(status);
    response.headers.extend([
        ("Content-Type".into(), "text/html; charset=utf-8".into()),
        (
            "Content-Security-Policy".into(),
            "default-src 'none'; style-src 'unsafe-inline'; form-action 'self'; frame-ancestors 'none'".into(),
        ),
        ("X-Frame-Options".into(), "DENY".into()),
        ("Referrer-Policy".into(), "no-referrer".into()),
    ]);
    response.body = format!(
        "<!doctype html><html lang=\"zh-Hans\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width\"><title>Enouia Memory</title></head><body style=\"font-family:sans-serif;max-width:32rem;margin:2rem auto;padding:0 1rem\">{body}</body></html>"
    )
    .into_bytes();
    response
}

impl<'a> Authority<'a> {
    pub fn new(
        config: OAuthConfig,
        ids: &'a dyn IdSource,
        clock: &'a dyn Clock,
        state: State,
    ) -> Self {
        Self {
            config,
            ids,
            clock,
            state: RefCell::new(state),
        }
    }

    /// Replace the state, e.g. with the persisted copy before each request.
    pub fn replace_state(&self, state: State) {
        *self.state.borrow_mut() = state;
    }

    /// The state to persist after any response; it holds digests only.
    pub fn state(&self) -> State {
        self.state.borrow().clone()
    }

    fn secret(&self) -> String {
        let mut bytes = self.ids.random_16().to_vec();
        bytes.extend(self.ids.random_16());
        base64url(&bytes)
    }

    /// A fresh one-time approval code for the owner, replacing any earlier one.
    pub fn issue_approval_code(&self) -> String {
        let code: String = self.ids.random_16()[6..]
            .iter()
            .map(|b| APPROVAL_ALPHABET[(*b & 31) as usize] as char)
            .collect();
        self.state.borrow_mut().approval = Some(Approval {
            digest: digest(&code),
            expires_ms: self.clock.now_unix_ms() + APPROVAL_TTL_MS,
            failures: 0,
        });
        code
    }

    fn prune(&self, now: i64) {
        let mut state = self.state.borrow_mut();
        state.codes.retain(|_, g| g.expires_ms > now);
        state.access.retain(|_, t| t.expires_ms > now);
        state.refresh.retain(|_, t| t.expires_ms > now);
        state.spent.retain(|_, t| t.expires_ms > now);
        if state.approval.as_ref().is_some_and(|a| a.expires_ms <= now) {
            state.approval = None;
        }
    }

    fn revoke(&self, family: &str) {
        let mut state = self.state.borrow_mut();
        state.codes.retain(|_, g| g.family != family);
        state.access.retain(|_, t| t.family != family);
        state.refresh.retain(|_, t| t.family != family);
    }
    /// Native Owner administration only; no HTTP revocation surface.
    pub fn revoke_client(&self, client_id: &str) -> bool {
        let mut state = self.state.borrow_mut();
        let existed = state.clients.remove(client_id).is_some();
        state.codes.retain(|_, g| g.client_id != client_id);
        state.access.retain(|_, t| t.client_id != client_id);
        state.refresh.retain(|_, t| t.client_id != client_id);
        state.spent.retain(|_, t| t.client_id != client_id);
        if existed {
            state.approval = None;
        }
        existed
    }
    /// IDs and expiry metadata only; never digests, codes or tokens.
    pub fn client_status(&self) -> Value {
        let state = self.state.borrow();
        let now = self.clock.now_unix_ms();
        json!({"now_ms":now,"clients":state.clients.keys().map(|id| {
            json!({"client_id":id,
                "access_valid_until_ms":state.access.values().filter(|t| &t.client_id == id && t.expires_ms > now).map(|t| t.expires_ms).max(),
                "refresh_valid_until_ms":state.refresh.values().filter(|t| &t.client_id == id && t.expires_ms > now).map(|t| t.expires_ms).max()})
        }).collect::<Vec<_>>()})
    }

    /// Serve an OAuth path, or `None` when the request belongs elsewhere.
    pub fn respond(&self, request: &Request) -> Option<Response> {
        let (path, query) = request
            .target
            .split_once('?')
            .unwrap_or((&request.target, ""));
        if ![METADATA, REGISTER, AUTHORIZE, TOKEN].contains(&path) {
            return None;
        }
        let now = self.clock.now_unix_ms();
        self.prune(now);
        let header = |name: &str| {
            request
                .headers
                .iter()
                .find(|(n, _)| n.eq_ignore_ascii_case(name))
                .map(|(_, v)| v.as_str())
        };
        let is_form = header("content-type").is_some_and(|v| {
            v.split(';')
                .next()
                .unwrap_or("")
                .trim()
                .eq_ignore_ascii_case("application/x-www-form-urlencoded")
        });
        let body = || std::str::from_utf8(&request.body).ok().and_then(form);
        Some(match (request.method.as_str(), path) {
            ("GET", METADATA) if query.is_empty() => Response::json(
                200,
                &json!({
                    "issuer": self.config.issuer,
                    "authorization_endpoint": format!("{}{AUTHORIZE}", self.config.issuer),
                    "token_endpoint": format!("{}{TOKEN}", self.config.issuer),
                    "registration_endpoint": format!("{}{REGISTER}", self.config.issuer),
                    "response_types_supported": ["code"],
                    "grant_types_supported": ["authorization_code", "refresh_token"],
                    "code_challenge_methods_supported": ["S256"],
                    "token_endpoint_auth_methods_supported": ["none"],
                    "scopes_supported": [READ_SCOPE],
                }),
            ),
            ("POST", REGISTER) if query.is_empty() => self.register(request),
            ("GET", AUTHORIZE) => match form(query) {
                Some(params) => self.authorize(&params, None),
                None => page(400, "<p>授权请求格式无效。</p>"),
            },
            ("POST", AUTHORIZE) if query.is_empty() && is_form => match body() {
                Some(params) => self.authorize(&params, Some(now)),
                None => page(400, "<p>授权请求格式无效。</p>"),
            },
            ("POST", TOKEN) if query.is_empty() && is_form => match body() {
                Some(params) => self.token(&params, now),
                None => oauth_error(400, "invalid_request"),
            },
            _ => Response::new(405),
        })
    }

    fn register(&self, request: &Request) -> Response {
        let invalid = |error: &str| oauth_error(400, error);
        let Ok(Value::Object(meta)) = super::unique::parse(&request.body) else {
            return invalid("invalid_client_metadata");
        };
        let Some(uris) = meta.get("redirect_uris").and_then(Value::as_array) else {
            return invalid("invalid_redirect_uri");
        };
        let uris: Vec<String> = uris
            .iter()
            .filter_map(|u| u.as_str().map(str::to_owned))
            .collect();
        let allowed = |u: &String| {
            !u.contains('#')
                && self
                    .config
                    .redirect_prefixes
                    .iter()
                    .any(|p| u.starts_with(p.as_str()))
        };
        if uris.is_empty()
            || uris.len() > 4
            || uris.len() != meta["redirect_uris"].as_array().map_or(0, Vec::len)
            || !uris.iter().all(allowed)
        {
            return invalid("invalid_redirect_uri");
        }
        let strings = |key: &str| {
            meta.get(key).map(|v| {
                v.as_array()
                    .map(|a| a.iter().map(|x| x.as_str()).collect::<Vec<_>>())
            })
        };
        let method_ok = meta
            .get("token_endpoint_auth_method")
            .is_none_or(|m| m == "none");
        let grants_ok = match strings("grant_types") {
            None => true,
            Some(Some(g)) => {
                !g.is_empty()
                    && g.iter()
                        .all(|x| matches!(x, Some("authorization_code" | "refresh_token")))
            }
            Some(None) => false,
        };
        let responses_ok = match strings("response_types") {
            None => true,
            Some(Some(r)) => r == [Some("code")],
            Some(None) => false,
        };
        if !method_ok || !grants_ok || !responses_ok {
            return invalid("invalid_client_metadata");
        }
        let mut state = self.state.borrow_mut();
        if state.clients.len() >= MAX_CLIENTS {
            return invalid("invalid_client_metadata");
        }
        let client_id = hex(&self.ids.random_16());
        state.clients.insert(client_id.clone(), uris.clone());
        Response::json(
            201,
            &json!({
                "client_id": client_id,
                "client_id_issued_at": self.clock.now_unix_ms() / 1000,
                "redirect_uris": uris,
                "token_endpoint_auth_method": "none",
                "grant_types": ["authorization_code", "refresh_token"],
                "response_types": ["code"],
            }),
        )
    }

    /// GET shows the approval form; POST (with `now`) checks the owner's
    /// approval code and redirects with an authorization code.
    fn authorize(&self, params: &BTreeMap<String, String>, submitted: Option<i64>) -> Response {
        let get = |k: &str| params.get(k).map(String::as_str);
        let (Some(client_id), Some(redirect_uri)) = (get("client_id"), get("redirect_uri")) else {
            return page(400, "<p>缺少客户端或回调地址。</p>");
        };
        let registered = self
            .state
            .borrow()
            .clients
            .get(client_id)
            .is_some_and(|uris| uris.iter().any(|u| u == redirect_uri));
        if !registered {
            // Never redirect to an unverified address.
            return page(400, "<p>未知的客户端或回调地址。</p>");
        }
        let redirect = |pairs: &[(&str, &str)]| {
            let sep = if redirect_uri.contains('?') { '&' } else { '?' };
            let query: Vec<String> = pairs
                .iter()
                .chain(get("state").map(|s| ("state", s)).iter())
                .chain([("iss", self.config.issuer.as_str())].iter())
                .map(|(k, v)| format!("{k}={}", percent_encode(v)))
                .collect();
            let mut response = Response::new(302);
            response.headers.push((
                "Location".into(),
                format!("{redirect_uri}{sep}{}", query.join("&")),
            ));
            response
        };
        let challenge = get("code_challenge").unwrap_or("");
        let challenge_ok = (43..=128).contains(&challenge.len())
            && challenge
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        let scope_ok = get("scope").is_none_or(|s| s.split(' ').all(|x| x == READ_SCOPE));
        let resource_ok = get("resource").is_none_or(|r| r == self.config.resource);
        if get("response_type") != Some("code") {
            return redirect(&[("error", "unsupported_response_type")]);
        }
        if get("code_challenge_method") != Some("S256") || !challenge_ok {
            return redirect(&[("error", "invalid_request")]);
        }
        if !scope_ok {
            return redirect(&[("error", "invalid_scope")]);
        }
        if !resource_ok {
            return redirect(&[("error", "invalid_target")]);
        }
        let form = |message: &str| {
            let hidden: String = [
                "response_type",
                "client_id",
                "redirect_uri",
                "code_challenge",
                "code_challenge_method",
                "state",
                "scope",
                "resource",
            ]
            .iter()
            .filter_map(|k| {
                get(k).map(|v| {
                    format!(
                        "<input type=\"hidden\" name=\"{k}\" value=\"{}\">",
                        html_escape(v)
                    )
                })
            })
            .collect();
            page(
                if message.is_empty() { 200 } else { 400 },
                &format!(
                    "<h1>允许连接 Enouia Memory</h1><p>一个客户端请求只读访问你的记忆（回调：{}）。</p><p>在服务器上运行批准命令获取一次性批准码，然后填在这里。</p>{}<form method=\"post\" action=\"{AUTHORIZE}\">{hidden}<label>批准码 <input name=\"approval_code\" autocomplete=\"one-time-code\" required></label> <button type=\"submit\">允许</button></form>",
                    html_escape(redirect_uri),
                    if message.is_empty() {
                        String::new()
                    } else {
                        format!("<p role=\"alert\">{}</p>", html_escape(message))
                    }
                ),
            )
        };
        let Some(now) = submitted else {
            return form("");
        };
        let typed = get("approval_code")
            .unwrap_or("")
            .trim()
            .to_ascii_uppercase();
        let mut state = self.state.borrow_mut();
        let Some(approval) = state.approval.as_mut() else {
            return form("没有有效的批准码。请在服务器上重新生成一个。");
        };
        if approval.digest != digest(&typed) {
            approval.failures += 1;
            if approval.failures >= MAX_APPROVAL_FAILURES {
                state.approval = None;
                return form("批准码错误次数过多，已作废。请在服务器上重新生成一个。");
            }
            return form("批准码不正确。");
        }
        state.approval = None;
        let code = self.secret();
        state.codes.insert(
            digest(&code),
            Grant {
                client_id: client_id.to_owned(),
                redirect_uri: redirect_uri.to_owned(),
                challenge: challenge.to_owned(),
                family: hex(&self.ids.random_16()),
                expires_ms: now + CODE_TTL_MS,
            },
        );
        drop(state);
        redirect(&[("code", code.as_str())])
    }

    fn token(&self, params: &BTreeMap<String, String>, now: i64) -> Response {
        let get = |k: &str| params.get(k).map(String::as_str);
        if get("resource").is_some_and(|r| r != self.config.resource) {
            return oauth_error(400, "invalid_target");
        }
        if get("scope").is_some_and(|s| !s.split(' ').all(|x| x == READ_SCOPE)) {
            return oauth_error(400, "invalid_scope");
        }
        let Some(client_id) =
            get("client_id").filter(|c| self.state.borrow().clients.contains_key(*c))
        else {
            return oauth_error(401, "invalid_client");
        };
        let (family, used) = match get("grant_type") {
            Some("authorization_code") => {
                let (Some(code), Some(redirect_uri), Some(verifier)) =
                    (get("code"), get("redirect_uri"), get("code_verifier"))
                else {
                    return oauth_error(400, "invalid_request");
                };
                let key = digest(code);
                let mut state = self.state.borrow_mut();
                if let Some(spent) = state.spent.get(&key).cloned() {
                    drop(state);
                    self.revoke(&spent.family);
                    return oauth_error(400, "invalid_grant");
                }
                let Some(grant) = state.codes.remove(&key) else {
                    return oauth_error(400, "invalid_grant");
                };
                state.spent.insert(
                    key,
                    Issued {
                        client_id: grant.client_id.clone(),
                        family: grant.family.clone(),
                        expires_ms: now + REFRESH_TTL_MS,
                    },
                );
                let verifier_ok = (43..=128).contains(&verifier.len())
                    && verifier
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"-._~".contains(&b));
                let pkce = verifier_ok && {
                    let raw: Vec<u8> = (0..32)
                        .map(|i| {
                            u8::from_str_radix(
                                &sha256(verifier.as_bytes()).as_str()[2 * i..2 * i + 2],
                                16,
                            )
                            .unwrap_or(0)
                        })
                        .collect();
                    base64url(&raw) == grant.challenge
                };
                if grant.client_id != client_id || grant.redirect_uri != redirect_uri || !pkce {
                    return oauth_error(400, "invalid_grant");
                }
                (grant.family, None)
            }
            Some("refresh_token") => {
                let Some(refresh) = get("refresh_token") else {
                    return oauth_error(400, "invalid_request");
                };
                let key = digest(refresh);
                let mut state = self.state.borrow_mut();
                if let Some(spent) = state.spent.get(&key).cloned() {
                    drop(state);
                    self.revoke(&spent.family);
                    return oauth_error(400, "invalid_grant");
                }
                let Some(issued) = state.refresh.remove(&key) else {
                    return oauth_error(400, "invalid_grant");
                };
                if issued.client_id != client_id {
                    state.refresh.insert(key, issued);
                    return oauth_error(400, "invalid_grant");
                }
                (issued.family.clone(), Some((key, issued)))
            }
            _ => return oauth_error(400, "unsupported_grant_type"),
        };
        let mut state = self.state.borrow_mut();
        if let Some((key, issued)) = used {
            state.spent.insert(key, issued);
        }
        let access = self.secret();
        let refresh = self.secret();
        let issued = |ttl: i64| Issued {
            client_id: client_id.to_owned(),
            family: family.clone(),
            expires_ms: now + ttl,
        };
        state.access.insert(digest(&access), issued(ACCESS_TTL_MS));
        state
            .refresh
            .insert(digest(&refresh), issued(REFRESH_TTL_MS));
        Response::json(
            200,
            &json!({
                "access_token": access,
                "token_type": "Bearer",
                "expires_in": ACCESS_TTL_MS / 1000,
                "refresh_token": refresh,
                "scope": READ_SCOPE,
            }),
        )
        .header("Pragma", "no-cache".into())
    }
}

impl Tokens for Authority<'_> {
    fn verify(&self, token: &str) -> Option<Bearer> {
        let now = self.clock.now_unix_ms();
        let state = self.state.borrow();
        state
            .access
            .get(&digest(token))
            .filter(|t| t.expires_ms > now && state.clients.contains_key(&t.client_id))
            .map(|_| Bearer {
                subject: "owner".into(),
                scopes: vec![READ_SCOPE.into()],
            })
    }
}
