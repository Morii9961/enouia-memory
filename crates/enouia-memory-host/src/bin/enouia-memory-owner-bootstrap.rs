//! Explicit trusted local Owner command, never an MCP route.
#[cfg(windows)]
fn main() {
    match enouia_memory_host::owner_bootstrap::run_cli(
        &std::env::args().skip(1).collect::<Vec<_>>(),
    ) {
        Ok(value) => println!("{}", serde_json::to_string_pretty(&value).unwrap()),
        Err(error) => {
            eprintln!("owner bootstrap refused: {error:?}");
            std::process::exit(1);
        }
    }
}
#[cfg(not(windows))]
fn main() {
    eprintln!("Owner bootstrap requires its Windows native enrollment adapter");
    std::process::exit(1);
}
