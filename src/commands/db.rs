use crate::confirm::{confirm, confirm_prod, is_prod};
use crate::config::{read_config, resolve_server};
use crate::ssh::ssh_exec;
use crate::theme;
use anyhow::Result;
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
            let code = ssh_exec(
                server,
                "docker exec sitehaus-commerce-commerce-1 npx tsx scripts/seed.ts",
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
                r#"docker exec sitehaus-commerce-commerce-1 node -e "require('./packages/database/dist/migrate.js')""#,
            );
            std::process::exit(code);
        }

        DbCommand::Studio => {
            println!("Opening Drizzle Studio for {}...", theme::yellow(name));
            println!("Tunnel: localhost:5435 → {}:5432", theme::yellow(name));

            let mut ssh_args = vec![
                "-N".to_string(),
                "-L".to_string(),
                "5435:localhost:5432".to_string(),
            ];
            if let Some(key) = &server.ssh_key_path {
                ssh_args.push("-i".to_string());
                ssh_args.push(key.clone());
            }
            ssh_args.push(format!("{}@{}", server.ssh_user, server.host));

            let mut tunnel = Command::new("ssh").args(&ssh_args).spawn()?;

            std::thread::sleep(std::time::Duration::from_secs(1));

            println!("Tunnel open. Run in your commerce repo:");
            println!("  DATABASE_URL=postgresql://ecom:<password>@localhost:5435/ecommerce pnpm drizzle-kit studio");
            println!("\nPress Ctrl+C to close the tunnel.");

            tunnel.wait()?;
        }
    }

    Ok(())
}
