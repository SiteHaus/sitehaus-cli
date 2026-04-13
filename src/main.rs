mod commands;
mod confirm;
mod config;
mod ssh;
mod theme;

use anyhow::Result;
use owo_colors::OwoColorize;
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use clap::builder::styling::{AnsiColor, Color, Effects, RgbColor, Style, Styles};
use commands::{db::DbCommand, ops::OpsCommand, server::ServerCommand};

#[derive(Parser)]
#[command(name = "sitehaus", version, about = "SiteHaus server management CLI", disable_help_flag = true)]
struct Cli {
    /// Print help
    #[arg(short, long, global = true, action = clap::ArgAction::HelpLong)]
    help: Option<bool>,

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
    /// Check required env vars on the active server
    EnvCheck,
    /// Stream service logs
    Logs {
        /// Service name (ecom: gateway, commerce, payments, worker, caddy, postgres, redis)
        ///             (platform: api, web, dashboard, iam, commerce, caddy, postgres, redis)
        service: Option<String>,
    },
    /// Show running containers on the active server
    Ps,
    /// Restart one or more services (restarts all if none specified)
    Restart {
        /// Service(s) to restart
        services: Vec<String>,
    },
    /// Check the server health endpoint
    Health,
    /// Pull latest images and restart all services
    Deploy,
    /// Interactive first-run setup wizard
    Setup,
}

fn styles() -> Styles {
    let yellow = Style::new()
        .fg_color(Some(Color::Rgb(RgbColor(252, 244, 52))))
        .effects(Effects::BOLD);
    let white_bold = Style::new()
        .fg_color(Some(Color::Ansi(AnsiColor::White)))
        .effects(Effects::BOLD);
    let dimmed = Style::new().fg_color(Some(Color::Ansi(AnsiColor::BrightBlack)));

    Styles::styled()
        .header(white_bold.clone())
        .usage(white_bold)
        .literal(yellow)        // command names & flags
        .placeholder(dimmed.clone())
        .error(Style::new().fg_color(Some(Color::Ansi(AnsiColor::Red))).effects(Effects::BOLD))
        .valid(Style::new().fg_color(Some(Color::Ansi(AnsiColor::Green))))
        .invalid(dimmed)
}

fn main() -> Result<()> {
    let banner = format!("\n  {}\n", theme::gradient("sitehaus"));
    let cmd = Cli::command().styles(styles()).before_help(banner);
    let matches = cmd.get_matches();
    let cli = Cli::from_arg_matches(&matches)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let server_override = cli.server.as_deref();

    match &cli.command {
        Command::Server { cmd } => commands::server::run(cmd)?,

        Command::Use { name } => {
            let mut config = config::read_config()?;
            config::get_server(&config, name)?;
            config.active_server = Some(name.clone());
            config::write_config(&config)?;
            theme::success(&format!("Active server set to \"{}\".", theme::yellow(name)));
        }

        Command::Status => {
            let config = config::read_config()?;
            match &config.active_server {
                None => println!("No active server. Run: sitehaus use <server>"),
                Some(name) => {
                    let server = config::get_server(&config, name)?;
                    println!();
                    println!("  {}", theme::gradient(name));
                    println!("  {}  {}@{}", "→".dimmed(), server.ssh_user, server.host);
                    println!("  {}  {}", "→".dimmed(), server.health_url.dimmed());
                    println!();
                }
            }
        }

        Command::Db { cmd } => commands::db::run(cmd, server_override)?,

        Command::EnvCheck => commands::env::run(server_override)?,

        Command::Logs { service } => {
            commands::ops::run(&OpsCommand::Logs { service: service.clone() }, server_override)?
        }
        Command::Ps => commands::ops::run(&OpsCommand::Ps, server_override)?,
        Command::Restart { services } => {
            commands::ops::run(&OpsCommand::Restart { services: services.clone() }, server_override)?
        }
        Command::Health => commands::ops::run(&OpsCommand::Health, server_override)?,
        Command::Deploy => commands::ops::run(&OpsCommand::Deploy, server_override)?,
        Command::Setup => commands::setup::run()?,
    }

    Ok(())
}
