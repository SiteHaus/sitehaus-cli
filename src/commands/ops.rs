use crate::config::{read_config, resolve_server};
use crate::confirm::confirm;
use crate::ssh::ssh_exec;
use crate::theme;
use anyhow::Result;
use clap::Subcommand;
use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;

#[derive(Subcommand)]
pub enum OpsCommand {
    /// Stream logs from a service (or all services if unspecified)
    Logs {
        /// Service name: gateway, commerce, payments, worker, caddy, postgres, redis
        service: Option<String>,
    },
    /// Check the health endpoint of the active server
    Health,
    /// Pull latest images and restart all services
    Deploy,
}

pub fn run(cmd: &OpsCommand, server_override: Option<&str>) -> Result<()> {
    let config = read_config()?;
    let (name, server) = resolve_server(&config, server_override)?;

    match cmd {
        OpsCommand::Logs { service } => {
            const VALID_SERVICES: &[&str] =
                &["gateway", "commerce", "payments", "worker", "caddy", "postgres", "redis"];

            let remote_cmd = match service {
                Some(svc) => {
                    if !VALID_SERVICES.contains(&svc.as_str()) {
                        anyhow::bail!(
                            "unknown service \"{svc}\". Valid services: {}",
                            VALID_SERVICES.join(", ")
                        );
                    }
                    format!("docker logs sitehaus-commerce-{svc}-1 --tail 50 -f")
                }
                None => "cd /srv/sitehaus-commerce && docker compose -f docker-compose.prod.yml logs -f".to_string(),
            };
            let code = ssh_exec(server, &remote_cmd);
            std::process::exit(code);
        }

        OpsCommand::Health => {
            let spinner = ProgressBar::new_spinner();
            spinner.set_style(
                ProgressStyle::default_spinner()
                    .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
                    .template("{spinner} {msg}")
                    .unwrap(),
            );
            spinner.set_message(format!("Checking {}...", theme::yellow(name)));
            spinner.enable_steady_tick(Duration::from_millis(80));

            let url = &server.health_url;
            match ureq::get(url).call() {
                Ok(resp) => {
                    let status = resp.status();
                    spinner.finish_and_clear();
                    if status == 200 {
                        theme::success(&format!("{} is healthy", theme::yellow(name)));
                    } else {
                        theme::warn(&format!("{} returned status {status}", theme::yellow(name)));
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    spinner.finish_and_clear();
                    theme::error(&format!("{} is unreachable: {e}", theme::yellow(name)));
                    std::process::exit(1);
                }
            }
        }

        OpsCommand::Deploy => {
            confirm(&format!("Deploy to \"{}\"? This will pull latest images and restart all services.", theme::yellow(name)))?;
            println!("Deploying to {}...", theme::yellow(name));
            let code = ssh_exec(
                server,
                "cd /srv/sitehaus-commerce && docker compose -f docker-compose.prod.yml pull && docker compose -f docker-compose.prod.yml up -d --remove-orphans && docker compose -f docker-compose.prod.yml restart caddy && docker image prune -f",
            );
            std::process::exit(code);
        }
    }

    Ok(())
}
