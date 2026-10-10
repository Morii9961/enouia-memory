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

fn native_client(timeout: Duration) -> Result<reqwest::Client> {
    use rustls_platform_verifier::BuilderVerifierExt;
    // Own the crypto provider without installing/inheriting a process default.
    // Keep the same platform certificate/hostname verifier as pinned reqwest.
    let mut tls = rustls::ClientConfig::builder_with_provider(std::sync::Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|_| error(MemoryErrorCode::ProviderUnavailable))?
    .with_platform_verifier()
    .map_err(|_| error(MemoryErrorCode::ProviderUnavailable))?
    .with_no_client_auth();
    tls.alpn_protocols = vec![b"http/1.1".to_vec()];
    reqwest::Client::builder()
        .tls_backend_preconfigured(tls)
        .http1_only()
        .https_only(true)
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .timeout(timeout)
        .connect_timeout(timeout.min(Duration::from_secs(15)))
        .build()
        .map_err(|_| error(MemoryErrorCode::ProviderUnavailable))
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
        // HTTP media type/subtype tokens are ASCII case-insensitive.
        if !media_type.eq_ignore_ascii_case(expected) {
            return Err(error(MemoryErrorCode::ProviderUnavailable));
        }
        Ok(())
    }

    /// Collapse the received Content-Type field lines into the one value
    /// `validate_response_head` takes. RFC 9110 section 8.3 defines the
    /// field as a single value; readers that pick different repeated lines
    /// disagree about the body format. Absent, repeated or non-visible-ASCII
    /// fields yield `None`, which the head check refuses after status.
    pub fn single_content_type<'a>(fields: impl IntoIterator<Item = &'a [u8]>) -> Option<&'a str> {
        let mut fields = fields.into_iter();
        let (Some(value), None) = (fields.next(), fields.next()) else {
            return None;
        };
        // Same byte rule as the pinned `HeaderValue::to_str`.
        if !value
            .iter()
            .all(|&b| b == b'\t' || (0x20..0x7f).contains(&b))
        {
            return None;
        }
        std::str::from_utf8(value).ok()
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
            let client = native_client(timeout)?;
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
                let content_type = Self::single_content_type(response.headers()
                    .get_all(reqwest::header::CONTENT_TYPE).iter().map(|v| v.as_bytes()));
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

#[cfg(test)]
mod tests {
    use super::*;

    /// This crate's separate unit-test process starts with no global crypto
    /// provider. Build clients only: no requests, credentials or HTTP calls.
    #[test]
    fn native_client_neither_installs_nor_inherits_process_crypto_defaults() {
        assert!(rustls::crypto::CryptoProvider::get_default().is_none());
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let client = native_client(Duration::from_secs(1)).unwrap();
            assert!(rustls::crypto::CryptoProvider::get_default().is_none());
            drop(client);

            // A deliberately unusable synthetic host default must not alter
            // this transport's client configuration or be overwritten by it.
            let mut host = rustls::crypto::ring::default_provider();
            host.cipher_suites.clear();
            host.install_default().unwrap();
            let installed = rustls::crypto::CryptoProvider::get_default()
                .unwrap()
                .clone();
            let first = native_client(Duration::from_secs(1)).unwrap();
            let second = native_client(Duration::from_secs(1)).unwrap();
            assert!(std::sync::Arc::ptr_eq(
                &installed,
                rustls::crypto::CryptoProvider::get_default().unwrap()
            ));
            assert!(installed.cipher_suites.is_empty());
            drop((first, second));
        });
    }

    /// Pinned hyper appends each received field line; `HeaderMap::get`
    /// returns only the first, so a later contradicting line went unseen.
    #[test]
    fn repeated_content_type_lines_are_refused_after_status_classification() {
        use reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderValue};
        fn lines(values: &[&[u8]]) -> HeaderMap {
            let mut headers = HeaderMap::new();
            for value in values {
                headers.append(CONTENT_TYPE, HeaderValue::from_bytes(value).unwrap());
            }
            headers
        }
        fn received(headers: &HeaderMap) -> Option<&str> {
            HttpsTransport::single_content_type(
                headers.get_all(CONTENT_TYPE).iter().map(|v| v.as_bytes()),
            )
        }
        for (streaming, expected, other) in [
            (false, "application/json", "text/html"),
            (true, "text/event-stream", "application/json"),
        ] {
            let mixed = lines(&[expected.as_bytes(), other.as_bytes()]);
            // The hazard: first-line reads accept the expected format.
            let first = mixed.get(CONTENT_TYPE).unwrap().to_str().ok();
            HttpsTransport::validate_response_head(200, first, streaming).unwrap();

            let single = lines(&[expected.as_bytes()]);
            assert_eq!(received(&single), Some(expected));
            HttpsTransport::validate_response_head(200, received(&single), streaming).unwrap();
            for values in [
                &[][..],
                &[expected.as_bytes(), other.as_bytes()][..],
                &[other.as_bytes(), expected.as_bytes()][..],
                &[expected.as_bytes(), expected.as_bytes()][..],
                &[expected.as_bytes(), b""][..],
                &[b"", expected.as_bytes()][..],
                &[expected.as_bytes(); 3][..],
                &[&b"application/j\x80son"[..]][..],
                &["application/jſon".as_bytes()][..],
            ] {
                let headers = lines(values);
                assert_eq!(received(&headers), None);
                for (status, code) in [
                    (200, MemoryErrorCode::ProviderUnavailable),
                    (401, MemoryErrorCode::Unauthenticated),
                    (403, MemoryErrorCode::PermissionDenied),
                    (429, MemoryErrorCode::ProviderUnavailable),
                ] {
                    assert_eq!(
                        HttpsTransport::validate_response_head(
                            status,
                            received(&headers),
                            streaming
                        )
                        .unwrap_err()
                        .code,
                        code
                    );
                }
            }
        }
        // Tab and visible ASCII are the only accepted value bytes, matching
        // the pinned `HeaderValue::to_str` used before this check.
        assert_eq!(
            HttpsTransport::single_content_type([&b"\tapplication/json; charset=utf-8\t"[..]]),
            Some("\tapplication/json; charset=utf-8\t")
        );
        for value in [
            &b"application/json\x7f"[..],
            b"application/\x01json",
            b"\xc3\xa9",
        ] {
            assert_eq!(HttpsTransport::single_content_type([value]), None);
        }
    }
}
