use crate::config::{read_config, resolve_server};
use crate::confirm::confirm;
use crate::ssh::ssh_exec;
use anyhow::Result;
use clap::Subcommand;

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
            println!("Checking health for {name}...");
            let url = &server.health_url;
            match ureq::get(url).call() {
                Ok(resp) => {
                    let status = resp.status();
                    if status == 200 {
                        println!("✓  {name} is healthy ({status})");
                    } else {
                        println!("⚠  {name} returned status {status}");
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    println!("✗  {name} is unreachable: {e}");
                    std::process::exit(1);
                }
            }
        }

        OpsCommand::Deploy => {
            confirm(&format!("Deploy to \"{name}\"? This will pull latest images and restart all services."))?;
            println!("Deploying to {name}...");
            let code = ssh_exec(
                server,
                "cd /srv/sitehaus-commerce && docker compose -f docker-compose.prod.yml pull && docker compose -f docker-compose.prod.yml up -d --remove-orphans && docker compose -f docker-compose.prod.yml restart caddy && docker image prune -f",
            );
            std::process::exit(code);
        }
    }

    Ok(())
}
