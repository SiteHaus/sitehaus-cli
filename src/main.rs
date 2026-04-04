mod commands;
mod confirm;
mod config;
mod ssh;

use anyhow::Result;
use clap::{Parser, Subcommand};
use commands::{db::DbCommand, server::ServerCommand};

#[derive(Parser)]
#[command(name = "sitehaus", version, about = "SiteHaus server management CLI")]
struct Cli {
    /// Override the active server for this command
    #[arg(long, global = true)]
    server: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Manage registered servers
    Server {
        #[command(subcommand)]
        cmd: ServerCommand,
    },
    /// Set the active server
    Use {
        /// Server name to activate
        name: String,
    },
    /// Show active server and health status
    Status,
    /// Database operations
    Db {
        #[command(subcommand)]
        cmd: DbCommand,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let server_override = cli.server.as_deref();

    match &cli.command {
        Command::Server { cmd } => commands::server::run(cmd)?,

        Command::Use { name } => {
            let mut config = config::read_config()?;
            config::get_server(&config, name)?;
            config.active_server = Some(name.clone());
            config::write_config(&config)?;
            println!("✓  Active server set to \"{name}\".");
        }

        Command::Status => {
            let config = config::read_config()?;
            match &config.active_server {
                None => println!("No active server. Run: sitehaus use <server>"),
                Some(name) => {
                    let server = config::get_server(&config, name)?;
                    println!(
                        "Active server: {name}  ({}@{})",
                        server.ssh_user, server.host
                    );
                    println!("Health URL:    {}", server.health_url);
                }
            }
        }

        Command::Db { cmd } => commands::db::run(cmd, server_override)?,
    }

    Ok(())
}
