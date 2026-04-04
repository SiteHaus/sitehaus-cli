use crate::confirm::{confirm, confirm_prod, is_prod};
use crate::config::{read_config, resolve_server};
use crate::ssh::{ssh_capture, ssh_exec};
use crate::theme;
use anyhow::{Context, Result};
use clap::Subcommand;
use std::process::Command;

#[derive(Subcommand)]
pub enum DbCommand {
    /// Seed the database with dev data
    Seed,
    /// Run database migrations
    Migrate,
    /// Open Drizzle Studio via SSH tunnel
    Studio,
}

pub fn run(cmd: &DbCommand, server_override: Option<&str>) -> Result<()> {
    let config = read_config()?;
    let (name, server) = resolve_server(&config, server_override)?;

    match cmd {
        DbCommand::Seed => {
            if is_prod(name) {
                confirm_prod(name)?;
            } else {
                confirm(&format!(
                    "Seed database on \"{}\"? This will wipe and re-insert seed data.",
                    theme::yellow(name)
                ))?;
            }

            println!("Seeding database on {}...", theme::yellow(name));
            // Copy scripts from server repo into container (not baked into image), then run
            let code = ssh_exec(
                server,
                "docker cp /srv/sitehaus-commerce/scripts sitehaus-commerce-commerce-1:/app/ && docker exec sitehaus-commerce-commerce-1 sh -c 'cd /app && npx tsx scripts/seed.ts'",
            );
            std::process::exit(code);
        }

        DbCommand::Migrate => {
            if is_prod(name) {
                confirm_prod(name)?;
            } else {
                confirm(&format!("Run migrations on \"{}\"?", theme::yellow(name)))?;
            }

            println!("Running migrations on {}...", theme::yellow(name));
            let code = ssh_exec(
                server,
                "docker exec sitehaus-commerce-commerce-1 sh -c 'cd /app/packages/database && ./node_modules/.bin/drizzle-kit migrate'",
            );
            std::process::exit(code);
        }

        DbCommand::Studio => {
            let local_path = server
                .local_path
                .as_deref()
                .context("no local_path set for this server — re-run: sitehaus setup")?;

            println!("Fetching DATABASE_URL from {}...", theme::yellow(name));
            let raw_url = ssh_capture(
                server,
                "docker exec sitehaus-commerce-commerce-1 printenv DATABASE_URL",
            )?;
            if raw_url.is_empty() {
                anyhow::bail!("could not fetch DATABASE_URL from container");
            }

            // Rewrite the host:port so it points through the tunnel
            // e.g. postgresql://user:pass@postgres:5432/db → @localhost:5435/db
            let db_url = rewrite_db_url(&raw_url, "localhost", 5435);

            println!("Opening tunnel: localhost:5435 → {}:5432", theme::yellow(name));
            let mut ssh_args = vec![
                "-N".to_string(),
                "-L".to_string(),
                "5435:localhost:5432".to_string(),
                "-o".to_string(),
                "LogLevel=ERROR".to_string(),
            ];
            if let Some(key) = &server.ssh_key_path {
                ssh_args.push("-i".to_string());
                ssh_args.push(key.clone());
            }
            ssh_args.push(format!("{}@{}", server.ssh_user, server.host));

            let mut tunnel = Command::new("ssh").args(&ssh_args).spawn()?;
            std::thread::sleep(std::time::Duration::from_secs(1));

            let studio_dir = std::path::Path::new(local_path).join("packages/database");
            println!("Launching Drizzle Studio in {}...", studio_dir.display());

            let studio_status = Command::new("pnpm")
                .args(["drizzle-kit", "studio"])
                .current_dir(&studio_dir)
                .env("DATABASE_URL", &db_url)
                .status();

            let _ = tunnel.kill();
            let _ = tunnel.wait();

            match studio_status {
                Ok(s) if s.success() => {}
                Ok(s) => std::process::exit(s.code().unwrap_or(1)),
                Err(e) => return Err(e.into()),
            }
        }
    }

    Ok(())
}

/// Rewrite the host:port in a postgres URL to point through the SSH tunnel.
/// Handles `postgresql://user:pass@host:port/db` and `postgres://...` schemes.
fn rewrite_db_url(url: &str, new_host: &str, new_port: u16) -> String {
    // Find the '@' that separates credentials from host
    if let Some(at) = url.rfind('@') {
        let prefix = &url[..=at]; // includes the '@'
        let rest = &url[at + 1..]; // host:port/db...
        // strip existing host:port (everything before the first '/')
        let path = if let Some(slash) = rest.find('/') {
            &rest[slash..]
        } else {
            ""
        };
        format!("{}{}:{}{}", prefix, new_host, new_port, path)
    } else {
        url.to_string()
    }
}
