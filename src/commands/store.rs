use crate::config::{get_server, read_config, resolve_server, ServerType};
use crate::ssh::ssh_capture;
use crate::theme;
use anyhow::{Context, Result};
use clap::Subcommand;
use owo_colors::OwoColorize;

use super::db::psql_capture;

#[derive(Subcommand)]
pub enum StoreCommand {
    /// Validate the full setup chain for a store: DB records, IAM link, and live HTTP resolution
    Check {
        /// Store slug to validate (e.g. onehealthclinics)
        slug: String,
        /// Platform server to validate the IAM client against (e.g. platform-staging)
        #[arg(long)]
        platform_server: String,
    },
}

pub fn run(cmd: &StoreCommand, server_override: Option<&str>) -> Result<()> {
    match cmd {
        StoreCommand::Check { slug, platform_server } => {
            let config = read_config()?;
            let (ecom_name, ecom_server) = resolve_server(&config, server_override)?;

            match ecom_server.server_type {
                ServerType::Platform => {
                    anyhow::bail!("store check requires an ecom server — use --server or sitehaus use <ecom-server>")
                }
                ServerType::Ecom => {}
            }

            let tick  = "✓".green().bold().to_string();
            let cross = "✗".red().bold().to_string();
            let warn  = "⚠".yellow().bold().to_string();

            println!(
                "\n  Checking store {} on {}...\n",
                theme::yellow(slug),
                theme::yellow(ecom_name)
            );

            let mut all_ok = true;

            // ── Step 1: Commerce DB ─────────────────────────────────────────────
            println!("  {} Commerce DB  ({})", "→".dimmed(), ecom_name.dimmed());

            let store_row = psql_capture(
                ecom_server,
                &format!("SELECT id, client_id, domain, name FROM stores WHERE slug = '{slug}'"),
            )?;

            if store_row.trim().is_empty() {
                println!(
                    "  {cross}  {:<24} no store with slug \"{slug}\"",
                    "store not found"
                );
                println!(
                    "         {}",
                    format!("Run: sitehaus db provision {slug} --client-key <key> --platform-server <name>").dimmed()
                );
                println!();
                theme::error(&format!("Store \"{slug}\" is not provisioned on \"{ecom_name}\"."));
                println!();
                return Ok(());
            }

            // psql -t -A outputs: col1|col2|col3|...
            let parts: Vec<&str> = store_row.trim().splitn(4, '|').collect();
            let (store_client_id, store_domain, store_name) = match parts.as_slice() {
                [_id, cid, dom, name] => (*cid, *dom, *name),
                _ => anyhow::bail!("unexpected store query output: {store_row}"),
            };

            println!(
                "  {tick}  {:<24} name={}, domain={}",
                "store found",
                store_name.bold(),
                store_domain
            );
            println!("  {tick}  {:<24} {}", "client_id", store_client_id.dimmed());
            println!();

            // ── Step 2: Platform IAM ────────────────────────────────────────────
            let platform = get_server(&config, platform_server)
                .with_context(|| {
                    format!(
                        "platform server \"{platform_server}\" not found — \
                         run: sitehaus server list"
                    )
                })?;

            println!("  {} Platform IAM  ({})", "→".dimmed(), platform_server.dimmed());

            let client_row = psql_capture(
                platform,
                &format!(
                    "SELECT id, key, is_active FROM clients WHERE id = '{store_client_id}'"
                ),
            )?;

            if client_row.trim().is_empty() {
                println!(
                    "  {cross}  {:<24} id={store_client_id}",
                    "client not found"
                );
                println!(
                    "         {}",
                    "The store's client_id doesn't exist in IAM — re-provision with the correct --client-key".dimmed()
                );
                println!();
                theme::error(&format!(
                    "Store \"{slug}\" has a broken IAM link on \"{ecom_name}\"."
                ));
                println!();
                return Ok(());
            }

            let parts: Vec<&str> = client_row.trim().splitn(3, '|').collect();
            let (iam_client_id, client_key, client_active_raw) = match parts.as_slice() {
                [id, key, active] => (*id, *key, *active),
                _ => anyhow::bail!("unexpected client query output: {client_row}"),
            };

            let ids_match = iam_client_id == store_client_id;
            if ids_match {
                println!("  {tick}  {:<24} key={}", "client found", client_key.bold());
            } else {
                println!(
                    "  {cross}  {:<24} commerce={store_client_id} / IAM={iam_client_id}",
                    "client_id mismatch"
                );
                all_ok = false;
            }

            let is_active = client_active_raw == "t" || client_active_raw == "true";
            if is_active {
                println!("  {tick}  {:<24} true", "client active");
            } else {
                println!(
                    "  {warn}  {:<24} false — client is disabled in IAM",
                    "client active"
                );
                all_ok = false;
            }
            println!();

            // ── Step 3: Live HTTP resolution ────────────────────────────────────
            println!("  {} Live HTTP", "→".dimmed());

            // SSH curl through the gateway internally — /v1/products goes through
            // StoreResolutionMiddleware (unlike the raw Express /health handler).
            // Expected results:
            //   401/403 → store resolved, auth guard ran next (correct)
            //   200     → store resolved, products returned
            //   404 + "Store not found" → middleware rejected the slug
            let curl_cmd = format!(
                "STATUS=$(curl -s -o /tmp/sh-store-check.json -w '%{{http_code}}' \
                   -H 'X-Store-Slug: {slug}' http://localhost:7020/v1/products); \
                 BODY=$(cat /tmp/sh-store-check.json 2>/dev/null); \
                 echo \"$STATUS $BODY\""
            );

            let result = ssh_capture(ecom_server, &curl_cmd)?;
            let result = result.trim();
            let (status_str, body) = result
                .split_once(' ')
                .unwrap_or((result, ""));

            match status_str {
                "401" | "403" => {
                    println!(
                        "  {tick}  {:<24} {} (store resolved, auth guard ran)",
                        "GET /v1/products",
                        status_str.green()
                    );
                }
                "200" => {
                    println!(
                        "  {tick}  {:<24} {}",
                        "GET /v1/products",
                        "200 OK".green()
                    );
                }
                "404" if body.contains("Store not found") => {
                    println!(
                        "  {cross}  {:<24} 404 Store not found",
                        "GET /v1/products"
                    );
                    println!(
                        "         {}",
                        "DB records look correct but gateway returned 404 — try: sitehaus restart gateway".dimmed()
                    );
                    all_ok = false;
                }
                code => {
                    println!(
                        "  {warn}  {:<24} {} (unexpected — body: {})",
                        "GET /v1/products",
                        code,
                        body.chars().take(80).collect::<String>()
                    );
                    all_ok = false;
                }
            }

            println!();

            if all_ok {
                theme::success(&format!(
                    "Store \"{slug}\" is wired up correctly on \"{ecom_name}\"."
                ));
            } else {
                theme::error(&format!(
                    "Store \"{slug}\" has issues — see above."
                ));
            }

            println!();
            Ok(())
        }
    }
}
