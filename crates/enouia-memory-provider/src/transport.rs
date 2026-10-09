//! Native HTTPS only; fixed origins, certificate validation, no redirects,
//! proxy inference or automatic retry. Cancellation drops the request future.
use crate::{
    Result,
    codec::{ANTHROPIC_VERSION, Api, WireRequest},
    error,
};
use enouia_memory_contract::{MemoryErrorCode, foundation::Cancellation, ports::SecretBytes};
use std::time::Duration;

fn credential_text(secret: &SecretBytes) -> Result<&str> {
    let credential = std::str::from_utf8(secret.expose())
        .map_err(|_| error(MemoryErrorCode::Unauthenticated))?;
    if credential.is_empty()
        || credential.len() > 4096
        || credential.bytes().any(|b| b.is_ascii_control())
    {
        return Err(error(MemoryErrorCode::Unauthenticated));
    }
    Ok(credential)
}

pub trait Transport {
    /// Local execution-context validation only: no HTTP, credential reads or
    /// admission side effects. Existing/custom transports default to allowed.
    fn preflight(&self) -> Result<()> {
        Ok(())
    }
    /// Local secret-shape validation only. Never retain/log the bytes or
    /// contact a server. Custom protocols can keep opaque/binary credentials.
    fn validate_secret(&self, _secret: &SecretBytes) -> Result<()> {
        Ok(())
    }
    fn exchange(
        &self,
        request: &WireRequest,
        secret: &SecretBytes,
        cancellation: &dyn Cancellation,
        timeout: Duration,
        on_bytes: &mut dyn FnMut(&[u8]) -> Result<()>,
    ) -> Result<()>;
}

#[derive(Default)]
pub struct HttpsTransport;
impl HttpsTransport {
    /// Pure native response-head validation, shared with offline host harnesses.
    /// Invoke before reading a response body. This does not establish egress
    /// authorization, account access or final billing, and never carries text.
    pub fn validate_response_head(
        status: u16,
        content_type: Option<&str>,
        streaming: bool,
    ) -> Result<()> {
        if !(200..300).contains(&status) {
            return Err(error(match status {
                401 => MemoryErrorCode::Unauthenticated,
                403 => MemoryErrorCode::PermissionDenied,
                // Other statuses can have multiple service/billing causes.
                // Discard error bodies rather than infer a local budget or
                // record-level failure from provider-controlled messages.
                _ => MemoryErrorCode::ProviderUnavailable,
            }));
        }
        let media_type = content_type
            .unwrap_or("")
            .split(';')
            .next()
            .unwrap_or("")
            .trim();
        let expected = if streaming {
            "text/event-stream"
        } else {
            "application/json"
        };
        if media_type != expected {
            return Err(error(MemoryErrorCode::ProviderUnavailable));
        }
        Ok(())
    }
}
impl Transport for HttpsTransport {
    fn preflight(&self) -> Result<()> {
        // This synchronous native transport owns its runtime. Require a
        // dedicated thread outside ANY entered Tokio context, rather than
        // risk nested block_on panics or depend on private Tokio internals.
        if tokio::runtime::Handle::try_current().is_ok() {
            return Err(error(MemoryErrorCode::InvalidRequest));
        }
        Ok(())
    }
    fn validate_secret(&self, secret: &SecretBytes) -> Result<()> {
        credential_text(secret).map(|_| ())
    }
    fn exchange(
        &self,
        request: &WireRequest,
        secret: &SecretBytes,
        cancellation: &dyn Cancellation,
        timeout: Duration,
        on_bytes: &mut dyn FnMut(&[u8]) -> Result<()>,
    ) -> Result<()> {
        if cancellation.is_cancelled() {
            return Err(error(MemoryErrorCode::Cancelled));
        }
        if timeout.is_zero() || timeout > Duration::from_secs(300) {
            return Err(error(MemoryErrorCode::InvalidRequest));
        }
        self.preflight()?;
        let credential = credential_text(secret)?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| error(MemoryErrorCode::ProviderUnavailable))?;
        runtime.block_on(async {
            // ring is explicitly pinned; reqwest does not enable another crypto provider.
            let _ = rustls::crypto::ring::default_provider().install_default();
            let client = reqwest::Client::builder().https_only(true).no_proxy()
                .redirect(reqwest::redirect::Policy::none()).retry(reqwest::retry::never())
                .timeout(timeout).connect_timeout(timeout.min(Duration::from_secs(15)))
                .build().map_err(|_| error(MemoryErrorCode::ProviderUnavailable))?;
            let mut header = reqwest::header::HeaderValue::from_str(&match request.api {
                Api::OpenAiResponses => format!("Bearer {credential}"),
                Api::AnthropicMessages => credential.to_owned(),
            }).map_err(|_| error(MemoryErrorCode::Unauthenticated))?;
            header.set_sensitive(true);
            let mut builder = client.post(request.endpoint()).header(reqwest::header::CONTENT_TYPE, "application/json")
                .header(if request.streaming {reqwest::header::ACCEPT} else {reqwest::header::CONTENT_TYPE},
                    if request.streaming {"text/event-stream"} else {"application/json"})
                .body(request.body.clone());
            builder = match request.api {
                Api::OpenAiResponses => builder.header(reqwest::header::AUTHORIZATION, header),
                Api::AnthropicMessages => builder.header("x-api-key", header).header("anthropic-version", ANTHROPIC_VERSION),
            };
            let work = async {
                let mut response = builder.send().await.map_err(|_| error(MemoryErrorCode::ProviderUnavailable))?;
                // Error bodies may echo input/credentials; discard them entirely.
                let content_type = response.headers().get(reqwest::header::CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok());
                Self::validate_response_head(response.status().as_u16(), content_type, request.streaming)?;
                while let Some(bytes) = response.chunk().await.map_err(|_| error(MemoryErrorCode::ProviderUnavailable))? {
                    if cancellation.is_cancelled() { return Err(error(MemoryErrorCode::Cancelled)); }
                    on_bytes(&bytes)?;
                }
                Ok(())
            };
            tokio::pin!(work);
            let mut tick = tokio::time::interval(Duration::from_millis(25));
            loop {
                tokio::select! {
                    result = &mut work => return result,
                    _ = tick.tick() => if cancellation.is_cancelled() { return Err(error(MemoryErrorCode::Cancelled)); },
                }
            }
        })
    }
}
