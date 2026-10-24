//! Explicit protocol-only synthetic entry point; never opens a Vault or Host.
//! Run with --synthetic. Tools return backend_unavailable, never fake commits.
use enouia_memory_contract::common::{ActorRef, ActorType};
use enouia_memory_contract::ids::PrincipalId;
use enouia_memory_contract::ipc::{Arguments, IpcRequest};
use enouia_memory_contract::policy::Scope;
use enouia_memory_host::mcp::{AgentPort, Grant, PortFault, Server};
use serde_json::Value;

struct Synthetic;
impl AgentPort for Synthetic {
    fn call(&mut self, _: &ActorRef, _: &IpcRequest, _: Arguments) -> Result<Value, PortFault> {
        Err(PortFault::Unavailable)
    }
    fn disconnect(&mut self) {}
}
fn main() {
    if std::env::args().skip(1).collect::<Vec<_>>() != ["--synthetic"] {
        eprintln!("synthetic_mode_required");
        std::process::exit(2);
    }
    let grant = Grant::from_authenticated_agent(
        ActorRef {
            actor_type: ActorType::Agent,
            actor_id: PrincipalId::parse("prn_00000001-0000-4000-8000-000000000001")
                .expect("synthetic principal"),
        },
        vec![
            Scope::MemoryRead,
            Scope::SourceRead,
            Scope::ContextRead,
            Scope::MemoryPropose,
            Scope::SessionPropose,
        ],
    )
    .expect("synthetic grant");
    if Server::new(grant, Synthetic)
        .serve(&mut std::io::stdin().lock(), &mut std::io::stdout().lock())
        .is_err()
    {
        eprintln!("synthetic_stdio_failed");
        std::process::exit(2);
    }
}
