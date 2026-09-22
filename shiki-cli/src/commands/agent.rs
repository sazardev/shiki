//! `shiki agent status/connect/disconnect` — the CLI wrapper over
//! `shiki_core::agent_connect`. Deliberately doesn't go through
//! `Context::load()` (see `main.rs`'s dispatch, same treatment as
//! `doctor`/`extension`): none of this needs shiki's own config to be
//! valid, or even to exist.

use anyhow::{bail, Result};
use shiki_core::agent_connect::{self, AgentClient, Scope};

fn client_list() -> String {
    AgentClient::ALL
        .iter()
        .map(|c| c.id())
        .collect::<Vec<_>>()
        .join(", ")
}

fn parse_client(id: &str) -> Result<AgentClient> {
    AgentClient::from_id(id)
        .ok_or_else(|| anyhow::anyhow!("unknown agent client '{id}' — one of: {}", client_list()))
}

fn resolve_clients(client: Option<String>, all: bool) -> Result<Vec<AgentClient>> {
    match (client, all) {
        (Some(_), true) => bail!("pass either a client name or --all, not both"),
        (Some(id), false) => Ok(vec![parse_client(&id)?]),
        (None, true) => Ok(AgentClient::ALL.to_vec()),
        (None, false) => bail!("specify a client ({}) or --all", client_list()),
    }
}

/// `--project` forces project scope (erroring if the client doesn't
/// support it); otherwise each client's own default scope applies —
/// `Continue`'s only real scope is project, everyone else defaults to
/// user/global so shiki works regardless of which directory you're in.
fn scope_for(client: AgentClient, project: bool) -> Result<Scope> {
    let scope = if project {
        Scope::Project
    } else {
        agent_connect::default_scope(client)
    };
    if !agent_connect::supports_scope(client, scope) {
        bail!(
            "{} doesn't support {} scope",
            client.display_name(),
            if matches!(scope, Scope::Project) {
                "project"
            } else {
                "user"
            }
        );
    }
    Ok(scope)
}

pub fn status(project: bool) -> Result<()> {
    let cwd = std::env::current_dir()?;
    for client in AgentClient::ALL {
        let scope = match scope_for(client, project) {
            Ok(s) => s,
            Err(e) => {
                println!("{:<15} {e}", client.display_name());
                continue;
            }
        };
        let st = agent_connect::status(client, scope, &cwd)?;
        let state = if st.shiki_connected {
            "connected"
        } else if st.file_exists {
            "not connected"
        } else {
            "not configured"
        };
        println!(
            "{:<15} {:<15} {}",
            client.display_name(),
            state,
            st.config_path.display()
        );
    }
    let path_env = std::env::var("PATH").unwrap_or_default();
    match agent_connect::find_shiki_mcp_on_path(&path_env) {
        Some(p) => println!("\nshiki-mcp: found at {}", p.display()),
        None => println!("\nshiki-mcp: not found on PATH — run `cargo install shiki-mcp`"),
    }
    Ok(())
}

pub fn connect(client: Option<String>, all: bool, project: bool) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let clients = resolve_clients(client, all)?;
    let mut had_error = false;
    for c in clients {
        let scope = match scope_for(c, project) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{}: {e}", c.display_name());
                had_error = true;
                continue;
            }
        };
        match agent_connect::connect(c, scope, &cwd) {
            Ok(outcome) => {
                println!(
                    "{}: registered shiki-mcp in {}",
                    c.display_name(),
                    outcome.mcp_config_path.display()
                );
                println!(
                    "  usage guidance written to {}",
                    outcome.guidance_path.display()
                );
                if let Some(w) = outcome.warning {
                    println!("  warning: {w}");
                }
            }
            Err(e) => {
                eprintln!("{}: {e}", c.display_name());
                had_error = true;
            }
        }
    }
    if had_error {
        bail!("one or more clients failed to connect — see above");
    }
    Ok(())
}

pub fn disconnect(client: Option<String>, all: bool, project: bool) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let clients = resolve_clients(client, all)?;
    let mut had_error = false;
    for c in clients {
        let scope = match scope_for(c, project) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{}: {e}", c.display_name());
                had_error = true;
                continue;
            }
        };
        match agent_connect::disconnect(c, scope, &cwd) {
            Ok(outcome) => {
                if outcome.mcp_removed || outcome.guidance_removed {
                    println!("{}: disconnected", c.display_name());
                } else {
                    println!("{}: nothing to disconnect", c.display_name());
                }
            }
            Err(e) => {
                eprintln!("{}: {e}", c.display_name());
                had_error = true;
            }
        }
    }
    if had_error {
        bail!("one or more clients failed to disconnect — see above");
    }
    Ok(())
}

/// `shiki agent connect generic` — no file ever written, just the plain
/// MCP snippet to paste into whatever client isn't explicitly supported.
pub fn generic() -> Result<()> {
    let path_env = std::env::var("PATH").unwrap_or_default();
    let command = agent_connect::find_shiki_mcp_on_path(&path_env)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "/path/to/shiki-mcp".to_string());
    println!("Register shiki-mcp with any MCP client over stdio:\n");
    println!("{}", agent_connect::generic_snippet(&command));
    if command == "/path/to/shiki-mcp" {
        println!("\n(shiki-mcp not found on $PATH — replace the placeholder above once it's installed, or run `cargo install shiki-mcp`.)");
    }
    Ok(())
}
