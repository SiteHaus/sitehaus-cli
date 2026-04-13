use crate::config::{ServerConfig, ServerType, get_server, read_config, resolve_server};
use crate::confirm::{confirm, confirm_prod, is_prod};
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
    /// Run a SQL query and print results
    Query {
        /// SQL to execute (e.g. "SELECT id, name FROM clients")
        sql: String,
    },
    /// Provision a client store (ecom servers only)
    Provision {
        /// Client to provision: onehealth
        client: String,
        /// Storefront domain (e.g. onehealthclinics.com)
        #[arg(long)]
        domain: String,
        /// IAM client key (e.g. one-health) — ID is looked up from the platform server automatically
        #[arg(long)]
        client_key: String,
        /// Platform server name to resolve the client ID from (e.g. platform-staging)
        #[arg(long)]
        platform_server: String,
        /// Stripe Connect account ID (e.g. acct_xxx) — optional, can be added later
        #[arg(long)]
        stripe_account: Option<String>,
    },
}

/// Returns the primary app container name for a server type.
pub(crate) fn app_container(server_type: &ServerType) -> &'static str {
    match server_type {
        ServerType::Ecom => "sitehaus-commerce-commerce-1",
        ServerType::Platform => "sitehaus-api-1",
    }
}

/// Returns the postgres container name for a server type.
pub(crate) fn pg_container(server_type: &ServerType) -> &'static str {
    match server_type {
        ServerType::Ecom => "sitehaus-commerce-postgres-1",
        ServerType::Platform => "sitehaus-postgres-1",
    }
}

/// Run a SQL query against a server's database and return trimmed output.
/// Connects via Unix socket inside the postgres container to bypass pg_hba.conf TCP restrictions.
/// Uses `-t -A` for clean, unaligned output suitable for parsing.
pub(crate) fn psql_capture(server: &ServerConfig, sql: &str) -> Result<String> {
    let app = app_container(&server.server_type);
    let pg = pg_container(&server.server_type);
    let escaped = sql.replace('\'', "'\\''");
    let cmd = format!(
        "DB_URL=$(docker exec {app} printenv DATABASE_URL) && \
         PGUSER=$(echo $DB_URL | sed 's|.*://||;s|:.*||') && \
         PGPASSWORD=$(echo $DB_URL | sed 's|.*://[^:]*:||;s|@.*||') && \
         PGDATABASE=$(echo $DB_URL | sed 's|.*/||') && \
         docker exec \
           -e PGPASSWORD=\"$PGPASSWORD\" \
           {pg} \
           psql -h /var/run/postgresql -U \"$PGUSER\" -d \"$PGDATABASE\" -t -A -c '{escaped}'"
    );
    ssh_capture(server, &cmd)
}

pub fn run(cmd: &DbCommand, server_override: Option<&str>) -> Result<()> {
    let config = read_config()?;
    let (name, server) = resolve_server(&config, server_override)?;

    match cmd {
        DbCommand::Seed => {
            if is_prod(name) {
                confirm_prod(name)?;
            } else {
                confirm(&format!("Seed database on \"{}\"?", theme::yellow(name)))?;
            }

            println!("Seeding database on {}...", theme::yellow(name));

            let app = app_container(&server.server_type);
            let code = match server.server_type {
                ServerType::Ecom => ssh_exec(
                    server,
                    &format!(
                        "docker cp /srv/sitehaus-commerce/scripts {app}:/app/ && \
                         docker exec {app} sh -c 'cd /app && npx tsx scripts/seed.ts'"
                    ),
                ),
                ServerType::Platform => {
                    // Run via a temporary node container sharing the api container's network
                    // namespace so it can reach postgres without needing the compose network name.
                    ssh_exec(
                        server,
                        &format!(
                            "DB_URL=$(docker exec {app} printenv DATABASE_URL) && \
                             docker run --rm \
                               --network container:{app} \
                               -e DATABASE_URL=\"$DB_URL\" \
                               -v /srv/sitehaus:/app \
                               -w /app \
                               node:20-alpine \
                               sh -c 'corepack enable && corepack prepare pnpm@10.14.0 --activate && pnpm install --frozen-lockfile && pnpm turbo run build --filter=@site-haus/db... && pnpm --filter @site-haus/db db:seed'"
                        ),
                    )
                }
            };
            std::process::exit(code);
        }

        DbCommand::Migrate => {
            if is_prod(name) {
                confirm_prod(name)?;
            } else {
                confirm(&format!("Run migrations on \"{}\"?", theme::yellow(name)))?;
            }

            println!("Running migrations on {}...", theme::yellow(name));

            let app = app_container(&server.server_type);
            let code = match server.server_type {
                ServerType::Ecom => ssh_exec(
                    server,
                    &format!(
                        "DB_URL=$(docker exec {app} printenv DATABASE_URL) && \
                         docker run --rm \
                           --network container:{app} \
                           -e DATABASE_URL=\"$DB_URL\" \
                           -v /srv/sitehaus-commerce:/app \
                           -w /app \
                           node:20-alpine \
                           sh -c 'corepack enable && corepack prepare pnpm@10.14.0 --activate && pnpm install --frozen-lockfile && pnpm --filter @sitehaus-ecom/database db:migrate'"
                    ),
                ),
                ServerType::Platform => ssh_exec(
                    server,
                    &format!(
                        "DB_URL=$(docker exec {app} printenv DATABASE_URL) && \
                         docker run --rm \
                           --network container:{app} \
                           -e DATABASE_URL=\"$DB_URL\" \
                           -v /srv/sitehaus:/app \
                           -w /app \
                           node:20-alpine \
                           sh -c 'corepack enable && corepack prepare pnpm@10.14.0 --activate && pnpm install --frozen-lockfile && pnpm turbo run build --filter=@site-haus/db... && pnpm --filter @site-haus/db db:migrate'"
                    ),
                ),
            };
            std::process::exit(code);
        }

        DbCommand::Query { sql } => {
            // Escape single quotes in the SQL for the shell
            let escaped = sql.replace('\'', "'\\''");
            let app = app_container(&server.server_type);
            let pg = pg_container(&server.server_type);
            // Parse credentials from DATABASE_URL and connect via Unix socket inside
            // the postgres container — avoids pg_hba.conf TCP restrictions
            let remote_cmd = format!(
                "DB_URL=$(docker exec {app} printenv DATABASE_URL) && \
                 PGUSER=$(echo $DB_URL | sed 's|.*://||;s|:.*||') && \
                 PGPASSWORD=$(echo $DB_URL | sed 's|.*://[^:]*:||;s|@.*||') && \
                 PGDATABASE=$(echo $DB_URL | sed 's|.*/||') && \
                 docker exec \
                   -e PGPASSWORD=\"$PGPASSWORD\" \
                   {pg} \
                   psql -h /var/run/postgresql -U \"$PGUSER\" -d \"$PGDATABASE\" -c '{escaped}'"
            );
            let code = ssh_exec(server, &remote_cmd);
            std::process::exit(code);
        }

        DbCommand::Provision {
            client,
            domain,
            client_key,
            platform_server,
            stripe_account,
        } => {
            match server.server_type {
                ServerType::Platform => {
                    anyhow::bail!("provision is only supported on ecom servers")
                }
                ServerType::Ecom => {}
            }

            let script = match client.as_str() {
                "onehealth" => "provision-onehealth.ts",
                other => anyhow::bail!("unknown client \"{other}\" — supported: onehealth"),
            };

            // Resolve the IAM client ID from the platform server
            let platform = get_server(&config, platform_server).with_context(|| {
                format!("platform server \"{platform_server}\" not found in config")
            })?;

            println!(
                "Looking up client ID for key \"{}\" on {}...",
                theme::yellow(client_key),
                theme::yellow(platform_server)
            );
            let client_id = psql_capture(
                platform,
                &format!("SELECT id FROM clients WHERE key = '{client_key}'"),
            )?;
            let client_id = client_id.trim().to_string();
            if client_id.is_empty() {
                anyhow::bail!(
                    "no client found with key \"{client_key}\" on {platform_server} — \
                     check the key with: sitehaus db query --server {platform_server} \"SELECT id, key FROM clients\""
                );
            }

            let stripe_display = stripe_account
                .as_deref()
                .unwrap_or("none — payment disabled");
            confirm(&format!(
                "Provision \"{}\" on {} (domain: {}, client-id: {}, stripe: {})?",
                theme::yellow(client),
                theme::yellow(name),
                domain,
                client_id,
                stripe_display,
            ))?;

            println!(
                "Provisioning {} on {}...",
                theme::yellow(client),
                theme::yellow(name)
            );

            let stripe_env = match stripe_account.as_deref() {
                Some(acct) => format!("-e PROVISION_STRIPE_ACCOUNT={acct}"),
                None => String::new(),
            };

            let app = app_container(&server.server_type);
            let cmd = format!(
                "DB_URL=$(docker exec {app} printenv DATABASE_URL) && \
                 docker run --rm \
                   --network container:{app} \
                   -e DATABASE_URL=\"$DB_URL\" \
                   -e PROVISION_DOMAIN={domain} \
                   -e PROVISION_CLIENT_ID={client_id} \
                   {stripe_env} \
                   -v /srv/sitehaus-commerce:/app \
                   -w /app \
                   node:22-alpine \
                   sh -c 'cd /tmp && npm install --no-save pg tsx && npx tsx /app/scripts/{script}'"
            );
            let code = ssh_exec(server, &cmd);
            std::process::exit(code);
        }

        DbCommand::Studio => {
            let local_path = server
                .local_path
                .as_deref()
                .context("no local_path set for this server — re-run: sitehaus setup")?;

            println!("Fetching DATABASE_URL from {}...", theme::yellow(name));
            let app = app_container(&server.server_type);
            let raw_url = ssh_capture(server, &format!("docker exec {app} printenv DATABASE_URL"))?;
            if raw_url.is_empty() {
                anyhow::bail!("could not fetch DATABASE_URL from container");
            }

            // Get the postgres container's IP so the SSH tunnel can reach it directly.
            // The hostname in DATABASE_URL (e.g. "postgres") only resolves inside the
            // Docker network — not on the server's localhost.
            let pg = pg_container(&server.server_type);
            let pg_ip = ssh_capture(
                server,
                &format!(
                    "docker inspect {pg} --format='{{{{range .NetworkSettings.Networks}}}}{{{{.IPAddress}}}}{{{{end}}}}'",
                ),
            )?;
            let pg_ip = pg_ip.trim();
            if pg_ip.is_empty() {
                anyhow::bail!("could not determine postgres container IP");
            }

            // Rewrite the host:port so it points through the tunnel
            // e.g. postgresql://user:pass@postgres:5432/db → @localhost:5435/db
            let db_url = rewrite_db_url(&raw_url, "localhost", 5435);

            println!(
                "Opening tunnel: localhost:5435 → {}:5432 (via {})",
                theme::yellow(name),
                pg_ip
            );
            let tunnel_spec = format!("5435:{}:5432", pg_ip);
            let mut ssh_args = vec![
                "-N".to_string(),
                "-L".to_string(),
                tunnel_spec,
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
