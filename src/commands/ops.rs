use crate::config::{ServerType, read_config, resolve_server};
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
    /// Check the health endpoint of the active server
    Health,
    /// Pull latest images and restart all services
    Deploy,
}

// Ecom deploys the SAME compose file to both prod and staging (see
// env.rs's ECOM_COMPOSE), differentiated only by IMAGE_TAG — the CD
// pipeline's own deploy script sets this explicitly. Without exporting it
// here, `pull`/`up -d` on a staging box resolves ${IMAGE_TAG:-latest} and
// silently deploys the PRODUCTION image.
fn ecom_deploy_command(is_prod: bool) -> &'static str {
    if is_prod {
        "cd /srv/sitehaus-commerce && \
         docker compose -f docker-compose.prod.yml pull && \
         docker compose -f docker-compose.prod.yml up -d --remove-orphans && \
         docker compose -f docker-compose.prod.yml restart caddy && \
         docker image prune -f"
    } else {
        "cd /srv/sitehaus-commerce && \
         export IMAGE_TAG=staging && \
         docker compose -f docker-compose.prod.yml pull && \
         docker compose -f docker-compose.prod.yml up -d --remove-orphans && \
         docker compose -f docker-compose.prod.yml restart caddy && \
         docker image prune -f"
    }
}

pub fn run(cmd: &OpsCommand, server_override: Option<&str>) -> Result<()> {
    let config = read_config()?;
    let (name, server) = resolve_server(&config, server_override)?;
    let is_prod = crate::confirm::is_prod(name);

    match cmd {
        OpsCommand::Logs { service } => {
            let remote_cmd = match server.server_type {
                ServerType::Ecom => {
                    const VALID: &[&str] = &[
                        "gateway", "commerce", "payments", "worker", "caddy", "postgres", "redis",
                    ];
                    match service {
                        Some(svc) => {
                            if !VALID.contains(&svc.as_str()) {
                                anyhow::bail!("unknown service \"{svc}\". Valid services: {}", VALID.join(", "));
                            }
                            format!("docker logs sitehaus-commerce-{svc}-1 --tail 50 -f")
                        }
                        None => "cd /srv/sitehaus-commerce && docker compose -f docker-compose.prod.yml logs -f".to_string(),
                    }
                }
                ServerType::Platform => {
                    const VALID: &[&str] = &[
                        "api",
                        "web",
                        "dashboard",
                        "iam",
                        "commerce",
                        "caddy",
                        "postgres",
                        "redis",
                    ];
                    match service {
                        Some(svc) => {
                            if !VALID.contains(&svc.as_str()) {
                                anyhow::bail!("unknown service \"{svc}\". Valid services: {}", VALID.join(", "));
                            }
                            format!("docker logs sitehaus-{svc}-1 --tail 50 -f")
                        }
                        None => {
                            let compose_file = if is_prod { "docker-compose.prod.yml" } else { "docker-compose.staging.yml" };
                            format!("cd /srv/sitehaus && docker compose -f {compose_file} logs -f")
                        }
                    }
                }
            };
            let code = ssh_exec(server, &remote_cmd);
            std::process::exit(code);
        }

        OpsCommand::Ps => {
            let code = ssh_exec(
                server,
                "docker ps --format 'table {{.Names}}\\t{{.Status}}\\t{{.Image}}'",
            );
            std::process::exit(code);
        }

        OpsCommand::Restart { services } => {
            // sitehaus-commerce deploys ONE compose file to both commerce-prod and
            // commerce-staging (see .github/workflows/cd.yml), so Ecom needs no
            // is_prod branch. sitehaus deploys TWO — docker-compose.staging.yml to
            // sitehaus-staging, docker-compose.prod.yml to sitehaus-prod — so
            // Platform must branch, or a no-args restart against prod would run
            // staging's compose file (different image tags, different topology)
            // against the production containers.
            let (compose_file, repo) = match server.server_type {
                ServerType::Ecom => ("docker-compose.prod.yml", "/srv/sitehaus-commerce"),
                ServerType::Platform if is_prod => ("docker-compose.prod.yml", "/srv/sitehaus"),
                ServerType::Platform => ("docker-compose.staging.yml", "/srv/sitehaus"),
            };

            let remote_cmd = if services.is_empty() {
                format!("cd {repo} && docker compose -f {compose_file} restart")
            } else {
                // Fetch all running container names and match by substring
                let all = crate::ssh::ssh_capture(server, "docker ps --format '{{.Names}}'")?;
                let container_names: Vec<&str> = all.lines().collect();

                let mut to_restart: Vec<String> = Vec::new();
                for svc in services.iter() {
                    let matched: Vec<&str> = container_names
                        .iter()
                        .copied()
                        .filter(|n| n.contains(svc.as_str()))
                        .collect();
                    if matched.is_empty() {
                        anyhow::bail!(
                            "no running container matching \"{svc}\". Running containers:\n{}",
                            container_names.join("\n")
                        );
                    }
                    to_restart.extend(matched.iter().map(|s| s.to_string()));
                }

                format!("docker restart {}", to_restart.join(" "))
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
            confirm(&format!(
                "Deploy to \"{}\"? This will pull latest images and restart all services.",
                theme::yellow(name)
            ))?;
            println!("Deploying to {}...", theme::yellow(name));
            let cmd = match server.server_type {
                ServerType::Ecom => ecom_deploy_command(is_prod),
                ServerType::Platform => {
                    "cd /srv/sitehaus && \
                     git pull origin main && \
                     docker compose -f docker-compose.staging.yml pull && \
                     docker compose -f docker-compose.staging.yml up -d --remove-orphans && \
                     docker image prune -f"
                }
            };
            let code = ssh_exec(server, cmd);
            std::process::exit(code);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ecom_staging_deploy_pins_the_staging_image_tag() {
        // Regression: without this export, `pull`/`up -d` on a staging box
        // resolves ${IMAGE_TAG:-latest} and silently deploys production images.
        let cmd = ecom_deploy_command(false);
        assert!(cmd.contains("export IMAGE_TAG=staging"), "got: {cmd}");
    }

    #[test]
    fn ecom_prod_deploy_leaves_image_tag_unset() {
        let cmd = ecom_deploy_command(true);
        assert!(!cmd.contains("IMAGE_TAG"), "expected no IMAGE_TAG override on prod, got: {cmd}");
    }
}
