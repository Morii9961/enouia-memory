//! Native Host handshake metadata. None of these fields authenticate a peer.
use super::{HostDocument, sealed};
use crate::error::Violation;
use crate::hash::Sha256Hex;
use crate::host::PrimaryBinding;
use crate::ids::VaultId;
use crate::json::SchemaVersion;
use crate::store::StoreDocument;
use crate::time::Timestamp;
use crate::workspace::HostSurface;
use serde::{Deserialize, Serialize};

pub const HOST_PROTOCOL_VERSION: u32 = 1;
pub const WORKSPACE_PROTOCOL_VERSION: u32 = 1;
pub const MAX_HANDSHAKE_BYTES: usize = 4096;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientHello {
    pub schema_version: SchemaVersion,
    pub protocol_version: u32,
    pub workspace_version: u32,
    pub vault_id: VaultId,
    pub compatibility_hash: Sha256Hex,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerHello {
    pub schema_version: SchemaVersion,
    pub protocol_version: u32,
    pub workspace_version: u32,
    pub vault_id: VaultId,
    pub compatibility_hash: Sha256Hex,
    pub instance_id: Sha256Hex,
    pub primary: PrimaryBinding,
    pub surface: HostSurface,
    pub expires_at: Timestamp,
}

fn versions(protocol: u32, workspace: u32) -> Vec<Violation> {
    if protocol != HOST_PROTOCOL_VERSION || workspace != WORKSPACE_PROTOCOL_VERSION {
        vec![Violation::new("host.session_version", "")]
    } else {
        Vec::new()
    }
}
impl StoreDocument for ClientHello {
    fn validate(&self) -> Vec<Violation> {
        versions(self.protocol_version, self.workspace_version)
    }
}
impl StoreDocument for ServerHello {
    fn validate(&self) -> Vec<Violation> {
        versions(self.protocol_version, self.workspace_version)
    }
}
impl sealed::Sealed for ClientHello {}
impl sealed::Sealed for ServerHello {}
impl HostDocument for ClientHello {
    const MAX_BYTES: usize = MAX_HANDSHAKE_BYTES;
}
impl HostDocument for ServerHello {
    const MAX_BYTES: usize = MAX_HANDSHAKE_BYTES;
}
