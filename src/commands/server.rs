use crate::config::{read_config, write_config, ServerConfig, ServerType};
use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum ServerCommand {
    /// Register a new server
    Add {
        /// Name for this server (e.g. commerce-prod)
        name: String,
        /// Server type: ecom | platform
        #[arg(long)]
        r#type: String,
        /// SSH host (IP or domain)
        #[arg(long)]
        host: String,
        /// Repo path on the server
        #[arg(long)]
        repo: String,
        /// Health check URL
        #[arg(long)]
        health_url: String,
        /// SSH user (default: deploy)
        #[arg(long, default_value = "deploy")]
        ssh_user: String,
        /// Path to SSH private key
        #[arg(long)]
        ssh_key: Option<String>,
    },
    /// List registered servers
    List,
    /// Remove a registered server
    Remove {
        /// Server name to remove
        name: String,
    },
}

pub fn run(cmd: &ServerCommand) -> Result<()> {
    match cmd {
        ServerCommand::Add {
            name,
            r#type,
            host,
            repo,
            health_url,
            ssh_user,
            ssh_key,
        } => {
            let server_type = match r#type.as_str() {
                "ecom" => ServerType::Ecom,
                "platform" => ServerType::Platform,
                other => anyhow::bail!("unknown type \"{other}\" — must be ecom or platform"),
            };
            let mut config = read_config()?;
            config.servers.insert(
                name.clone(),
                ServerConfig {
                    server_type,
                    host: host.clone(),
                    ssh_user: ssh_user.clone(),
                    ssh_key_path: ssh_key.clone(),
                    repo_path: repo.clone(),
                    health_url: health_url.clone(),
                },
            );
            write_config(&config)?;
            println!("✓  Server \"{name}\" registered.");
        }

        ServerCommand::List => {
            let config = read_config()?;
            if config.servers.is_empty() {
                println!("No servers registered. Run: sitehaus server add");
                return Ok(());
            }
            for (name, s) in &config.servers {
                let active = config.active_server.as_deref() == Some(name.as_str());
                let marker = if active { "▶" } else { " " };
                println!("  {marker} {name}  ({})  {}@{}  {}",
                    match s.server_type { ServerType::Ecom => "ecom", ServerType::Platform => "platform" },
                    s.ssh_user, s.host, s.health_url
                );
            }
        }

        ServerCommand::Remove { name } => {
            let mut config = read_config()?;
            if config.servers.remove(name).is_none() {
                anyhow::bail!("server \"{name}\" not found");
            }
            if config.active_server.as_deref() == Some(name.as_str()) {
                config.active_server = None;
            }
            write_config(&config)?;
            println!("✓  Server \"{name}\" removed.");
        }
    }
    Ok(())
}
