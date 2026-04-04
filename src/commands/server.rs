use crate::config::{ServerConfig, ServerType, read_config, write_config};
use crate::theme;
use anyhow::Result;
use clap::Subcommand;
use tabled::{Table, Tabled, settings::{Style, object::Columns, Modify, Alignment}};

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
                    local_path: None,
                },
            );
            write_config(&config)?;
            theme::success(&format!("Server \"{}\" registered.", theme::yellow(name)));
        }

        ServerCommand::List => {
            let config = read_config()?;
            if config.servers.is_empty() {
                println!("No servers registered. Run: sitehaus server add");
                return Ok(());
            }

            #[derive(Tabled)]
            struct Row {
                #[tabled(rename = " ")]
                active: String,
                #[tabled(rename = "Name")]
                name: String,
                #[tabled(rename = "Type")]
                server_type: String,
                #[tabled(rename = "Host")]
                host: String,
                #[tabled(rename = "User")]
                ssh_user: String,
            }

            let mut rows: Vec<Row> = config.servers.iter().map(|(name, s)| {
                let is_active = config.active_server.as_deref() == Some(name.as_str());
                Row {
                    active: if is_active {
                        format!("{}", theme::yellow("▶"))
                    } else {
                        " ".to_string()
                    },
                    name: if is_active {
                        format!("{}", theme::yellow(name))
                    } else {
                        name.clone()
                    },
                    server_type: match s.server_type {
                        ServerType::Ecom => "ecom".to_string(),
                        ServerType::Platform => "platform".to_string(),
                    },
                    host: s.host.clone(),
                    ssh_user: s.ssh_user.clone(),
                }
            }).collect();
            rows.sort_by(|a, b| a.name.cmp(&b.name));

            let table = Table::new(rows)
                .with(Style::blank())
                .with(Modify::new(Columns::new(..)).with(Alignment::left()))
                .to_string();
            println!("{table}");
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
            theme::success(&format!("Server \"{}\" removed.", theme::yellow(name)));
        }
    }
    Ok(())
}
