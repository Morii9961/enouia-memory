//! ADR-MEM-229: the connector's own semantic index. It sends the text of the
//! memories the Agent may read, and the query, to an Ollama on this machine
//! (loopback only) and keeps the vectors in a cache file in the service's
//! owner-only state directory. Runtime keeps its own index and cache.
use crate::automation::semantic::SemanticIndex;
use enouia_memory_contract::next::{SemanticVector, TargetRef, digest};
use enouia_memory_vault::platform;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

/// The query waits at most this long; then the recall is lexical.
const QUERY_TIMEOUT: Duration = Duration::from_secs(3);
/// One batch of memory texts (the first run embeds the whole library).
const BATCH_TIMEOUT: Duration = Duration::from_secs(120);
const BATCH: usize = 16;
/// Keep the model loaded between recalls instead of Ollama's five minutes.
const KEEP_ALIVE: &str = "30m";

/// `"semantic": {"ollama": "127.0.0.1:11434", "model": "bge-m3"}` in the
/// service file. Absent keeps recall lexical.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticService {
    pub ollama: SocketAddr,
    pub model: String,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cache {
    model: String,
    vectors: BTreeMap<String, Vec<i16>>,
}

pub struct OllamaIndex {
    addr: SocketAddr,
    model: String,
    file: PathBuf,
    vectors: Mutex<BTreeMap<String, SemanticVector>>,
}

impl OllamaIndex {
    /// Refuses an address that is not on this machine: memory text never
    /// leaves it. A cache of another model is ignored.
    pub fn open(service: &SemanticService, state_dir: &Path) -> Result<Self, String> {
        if !service.ollama.ip().is_loopback() {
            return Err("semantic.ollama must be a loopback address (127.0.0.1 or ::1)".into());
        }
        if service.model.trim().is_empty() || service.model.len() > 128 {
            return Err("semantic.model must name the Ollama model".into());
        }
        let file = state_dir.join("semantic-cache.json");
        let cache: Cache = match std::fs::read(&file) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
            Err(_) => Cache::default(),
        };
        let vectors = if cache.model == service.model {
            cache
                .vectors
                .into_iter()
                .map(|(key, values)| {
                    (
                        key,
                        SemanticVector {
                            model: service.model.clone(),
                            values,
                        },
                    )
                })
                .filter(|(_, v)| v.valid())
                .collect()
        } else {
            BTreeMap::new()
        };
        Ok(Self {
            addr: service.ollama,
            model: service.model.clone(),
            file,
            vectors: Mutex::new(vectors),
        })
    }

    /// Embeds the memories the cache lacks and drops the vectors of
    /// memories no longer given (withdrawn, forgotten, out of scope).
    /// Returns how many were embedded; on an Ollama error the cache keeps
    /// what it has and the error is returned.
    pub fn refresh(&self, texts: &[(TargetRef, String)]) -> Result<usize, String> {
        let wanted: BTreeMap<String, &str> = texts
            .iter()
            .map(|(t, text)| (key(t), text.as_str()))
            .collect();
        let missing: Vec<(&String, &str)> = {
            let mut vectors = self.vectors.lock().map_err(|_| "cache lock")?;
            vectors.retain(|k, _| wanted.contains_key(k));
            wanted
                .iter()
                .filter(|(k, _)| !vectors.contains_key(*k))
                .map(|(k, t)| (k, *t))
                .collect()
        };
        let mut embedded = 0;
        let mut result = Ok(());
        for batch in missing.chunks(BATCH) {
            let inputs: Vec<&str> = batch.iter().map(|(_, t)| *t).collect();
            match self.embed(&inputs, BATCH_TIMEOUT) {
                Ok(found) => {
                    let mut vectors = self.vectors.lock().map_err(|_| "cache lock")?;
                    for ((k, _), v) in batch.iter().zip(found) {
                        vectors.insert((*k).clone(), v);
                        embedded += 1;
                    }
                }
                Err(e) => {
                    result = Err(e);
                    break;
                }
            }
        }
        self.save()?;
        result.map(|()| embedded)
    }

    pub fn cached(&self) -> usize {
        self.vectors.lock().map_or(0, |v| v.len())
    }

    fn save(&self) -> Result<(), String> {
        let cache = Cache {
            model: self.model.clone(),
            vectors: self
                .vectors
                .lock()
                .map_err(|_| "cache lock")?
                .iter()
                .map(|(k, v)| (k.clone(), v.values.clone()))
                .collect(),
        };
        let tmp = self.file.with_extension("json.tmp");
        let bytes = serde_json::to_vec(&cache).map_err(|e| e.to_string())?;
        let mut f = std::fs::File::create(&tmp).map_err(|e| format!("semantic cache: {e}"))?;
        f.write_all(&bytes)
            .and_then(|()| f.sync_all())
            .map_err(|e| format!("semantic cache: {e}"))?;
        drop(f);
        platform::replace_durable(&tmp, &self.file).map_err(|e| format!("semantic cache: {e}"))
    }

    fn embed(&self, inputs: &[&str], timeout: Duration) -> Result<Vec<SemanticVector>, String> {
        let body =
            json!({"model": self.model, "input": inputs, "keep_alive": KEEP_ALIVE}).to_string();
        let reply = post(self.addr, "/api/embed", &body, timeout)?;
        let value: Value =
            serde_json::from_slice(&reply).map_err(|e| format!("ollama reply: {e}"))?;
        let rows = value["embeddings"]
            .as_array()
            .ok_or("ollama reply without embeddings")?;
        if rows.len() != inputs.len() {
            return Err("ollama returned a different number of embeddings".into());
        }
        rows.iter()
            .map(|row| {
                let raw: Vec<f32> = row
                    .as_array()
                    .ok_or("ollama embedding is not a list")?
                    .iter()
                    .map(|x| x.as_f64().map(|x| x as f32).ok_or("ollama embedding value"))
                    .collect::<Result<_, _>>()?;
                SemanticVector::from_f32(&self.model, &raw)
                    .ok_or_else(|| "unusable ollama embedding".into())
            })
            .collect()
    }
}

impl SemanticIndex for OllamaIndex {
    fn model(&self) -> &str {
        &self.model
    }
    fn query(&self, text: &str) -> Option<SemanticVector> {
        self.embed(&[text], QUERY_TIMEOUT).ok()?.pop()
    }
    fn memory(&self, target: &TargetRef) -> Option<SemanticVector> {
        self.vectors.lock().ok()?.get(&key(target)).cloned()
    }
}

fn key(target: &TargetRef) -> String {
    digest(target).to_string()
}

/// One HTTP/1.1 POST to the loopback Ollama, closing the connection after
/// the reply. Only a 200 with a plain or chunked body is accepted.
fn post(addr: SocketAddr, path: &str, body: &str, timeout: Duration) -> Result<Vec<u8>, String> {
    let mut stream =
        TcpStream::connect_timeout(&addr, timeout).map_err(|e| format!("ollama connect: {e}"))?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|e| e.to_string())?;
    write!(
        stream,
        "POST {path} HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .map_err(|e| format!("ollama write: {e}"))?;
    // Ollama may keep the connection open after the reply, so stop at the
    // end of the body rather than at end of stream.
    let deadline = std::time::Instant::now() + timeout;
    let mut reply = Vec::new();
    let mut buf = [0; 64 * 1024];
    loop {
        if let Some(body) = complete(&reply)? {
            return Ok(body);
        }
        if std::time::Instant::now() > deadline {
            return Err("ollama read: timed out".into());
        }
        let n = stream
            .read(&mut buf)
            .map_err(|e| format!("ollama read: {e}"))?;
        if n == 0 {
            return complete(&reply)?.ok_or_else(|| "ollama closed before the reply ended".into());
        }
        reply.extend_from_slice(&buf[..n]);
    }
}

/// The body once the reply holds all of it, `None` while it does not.
fn complete(reply: &[u8]) -> Result<Option<Vec<u8>>, String> {
    let Some(split) = reply.windows(4).position(|w| w == b"\r\n\r\n") else {
        return Ok(None);
    };
    let head = String::from_utf8_lossy(&reply[..split]).to_lowercase();
    let body = &reply[split + 4..];
    if !head.starts_with("http/1.1 200") && !head.starts_with("http/1.0 200") {
        return Err(format!(
            "ollama status: {}",
            head.lines().next().unwrap_or("")
        ));
    }
    if !head.contains("transfer-encoding: chunked") {
        let length = head
            .lines()
            .find_map(|l| l.strip_prefix("content-length:"))
            .and_then(|v| v.trim().parse::<usize>().ok())
            .ok_or("ollama reply without length")?;
        return Ok((body.len() >= length).then(|| body[..length].to_vec()));
    }
    let mut out = Vec::new();
    let mut rest = body;
    loop {
        let Some(line) = rest.windows(2).position(|w| w == b"\r\n") else {
            return Ok(None);
        };
        let size = usize::from_str_radix(
            String::from_utf8_lossy(&rest[..line])
                .split(';')
                .next()
                .unwrap_or("")
                .trim(),
            16,
        )
        .map_err(|_| "ollama chunk size")?;
        rest = &rest[line + 2..];
        if size == 0 {
            return Ok(Some(out));
        }
        let Some(chunk) = rest.get(..size) else {
            return Ok(None);
        };
        out.extend_from_slice(chunk);
        let Some(next) = rest.get(size + 2..) else {
            return Ok(None);
        };
        rest = next;
    }
}
