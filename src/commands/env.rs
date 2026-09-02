use crate::config::{ServerType, read_config, resolve_server};
use crate::confirm::confirm;
use crate::ssh::{ssh_capture, ssh_exec};
use crate::theme;
use anyhow::Result;
use clap::Subcommand;
use owo_colors::OwoColorize;
use std::collections::HashMap;

// ─── Subcommands ──────────────────────────────────────────────────────────────

#[derive(Subcommand)]
pub enum EnvCommand {
    /// Check required env vars on the active server
    Check,
    /// Set an env var on the active server (KEY=VALUE)
    Set {
        /// Key-value pair to set, e.g. SESSION_SECRET=supersecret
        pair: String,
    },
}

// ─── Check rules ──────────────────────────────────────────────────────────────

struct EnvRule {
    key: &'static str,
    required: bool,
    container: &'static str,
    check: Option<fn(&str, is_prod: bool) -> Option<String>>,
}

const GW: &str = "sitehaus-commerce-gateway-1";
const COM: &str = "sitehaus-commerce-commerce-1";
const PAY: &str = "sitehaus-commerce-payments-1";

fn rules_for(server_type: &ServerType) -> Vec<EnvRule> {
    match server_type {
        ServerType::Ecom => vec![
            // ── Gateway ──────────────────────────────────────────────────────
            EnvRule { key: "DATABASE_URL",          required: true,  container: GW,  check: Some(check_no_localhost) },
            EnvRule { key: "REDIS_URL",             required: true,  container: GW,  check: Some(check_no_localhost) },
            EnvRule { key: "IAM_URL",               required: true,  container: GW,  check: Some(check_no_localhost) },
            EnvRule { key: "IAM_CLIENT_KEY",        required: true,  container: GW,  check: None },
            EnvRule { key: "SESSION_SECRET",        required: true,  container: GW,  check: Some(check_secret_length) },
            EnvRule { key: "PORT",                  required: false, container: GW,  check: None },
            // ── Payments ─────────────────────────────────────────────────────
            EnvRule { key: "STRIPE_SECRET_KEY",     required: false, container: PAY, check: Some(check_stripe_key) },
            EnvRule { key: "STRIPE_WEBHOOK_SECRET", required: false, container: PAY, check: None },
            // ── Commerce ─────────────────────────────────────────────────────
            EnvRule { key: "R2_ACCESS_KEY_ID",      required: false, container: COM, check: None },
            EnvRule { key: "R2_SECRET_ACCESS_KEY",  required: false, container: COM, check: None },
            EnvRule { key: "R2_BUCKET_NAME",        required: false, container: COM, check: None },
            EnvRule { key: "R2_CDN_URL",            required: false, container: COM, check: None },
            EnvRule { key: "EMAIL_FROM",            required: true,  container: COM, check: None },
            EnvRule { key: "EMAIL_DEV_REDIRECT",    required: false, container: COM, check: Some(check_dev_redirect_in_prod) },
            EnvRule { key: "CORS_ENFORCE",          required: true,  container: GW,  check: Some(check_cors_enforce) },
        ],
        ServerType::Platform => vec![
            EnvRule { key: "DATABASE_URL",    required: true,  container: "sitehaus-api-1", check: Some(check_no_localhost) },
            EnvRule { key: "JWT_SECRET",      required: true,  container: "sitehaus-api-1", check: Some(check_secret_length) },
            EnvRule { key: "ACCESS_TTL_SEC",  required: true,  container: "sitehaus-api-1", check: None },
            EnvRule { key: "REFRESH_TTL_SEC", required: true,  container: "sitehaus-api-1", check: None },
            EnvRule { key: "RESEND_API_KEY",  required: false, container: "sitehaus-api-1", check: None },
            EnvRule { key: "COOKIE_DOMAIN",   required: false, container: "sitehaus-api-1", check: Some(check_no_localhost) },
            EnvRule { key: "COOKIE_SAME_SITE",required: false, container: "sitehaus-api-1", check: None },
        ],
    }
}

// ─── Set targets ──────────────────────────────────────────────────────────────

struct ServiceTarget {
    label: &'static str,
    env_file: &'static str,
    container: &'static str,
    compose_file: &'static str,
}

// sitehaus-commerce deploys the SAME docker-compose.prod.yml to both
// commerce-prod and commerce-staging (see .github/workflows/cd.yml) — no
// staging-specific compose file exists there, so one constant is correct.
const ECOM_COMPOSE: &str = "/srv/sitehaus-commerce/docker-compose.prod.yml";

// sitehaus, unlike sitehaus-commerce, deploys TWO DIFFERENT compose files —
// docker-compose.staging.yml to sitehaus-staging, docker-compose.prod.yml to
// sitehaus-prod (see .github/workflows/cd.yml stages 2 and 5). They're not
// interchangeable: staging's `commerce` service has no env_file at all
// (it's a build-time-baked Next.js frontend there), while prod's declares
// `./apps/commerce/.env`. Running the prod compose file against the staging
// box makes `docker compose up -d <anything>` try to parse prod's full
// service list — including that env_file prod expects but staging never
// created — and fail before the target service even restarts.
const PLATFORM_COMPOSE_PROD:    &str = "/srv/sitehaus/docker-compose.prod.yml";
const PLATFORM_COMPOSE_STAGING: &str = "/srv/sitehaus/docker-compose.staging.yml";

fn targets_for_key<'a>(key: &str, server_type: &ServerType, is_prod: bool) -> Vec<ServiceTarget> {
    match server_type {
        ServerType::Ecom => match key {
            "STRIPE_SECRET_KEY" | "STRIPE_WEBHOOK_SECRET" => vec![
                ServiceTarget { label: "payments", env_file: "/srv/sitehaus-commerce/apps/payments/.env", container: PAY, compose_file: ECOM_COMPOSE },
            ],
            "R2_ACCESS_KEY_ID" | "R2_SECRET_ACCESS_KEY" | "R2_BUCKET_NAME"
            | "R2_CDN_URL" | "R2_ACCOUNT_ID" => vec![
                ServiceTarget { label: "commerce", env_file: "/srv/sitehaus-commerce/apps/commerce/.env", container: COM, compose_file: ECOM_COMPOSE },
            ],
            // EMAIL_FROM and RESEND_API_KEY are consumed by both commerce and worker
            "EMAIL_FROM" | "RESEND_API_KEY" => vec![
                ServiceTarget { label: "commerce", env_file: "/srv/sitehaus-commerce/apps/commerce/.env", container: COM, compose_file: ECOM_COMPOSE },
                ServiceTarget { label: "worker",   env_file: "/srv/sitehaus-commerce/apps/worker/.env",   container: "sitehaus-commerce-worker-1", compose_file: ECOM_COMPOSE },
            ],
            "EMAIL_DEV_REDIRECT" => vec![
                ServiceTarget { label: "worker", env_file: "/srv/sitehaus-commerce/apps/worker/.env", container: "sitehaus-commerce-worker-1", compose_file: ECOM_COMPOSE },
            ],
            // Everything else (IAM_URL, IAM_CLIENT_KEY, SESSION_SECRET, DATABASE_URL, REDIS_URL, PORT, …) → gateway
            _ => vec![
                ServiceTarget { label: "gateway", env_file: "/srv/sitehaus-commerce/apps/gateway/.env", container: GW, compose_file: ECOM_COMPOSE },
            ],
        },
        ServerType::Platform => {
            let compose_file = if is_prod { PLATFORM_COMPOSE_PROD } else { PLATFORM_COMPOSE_STAGING };
            match key {
                // Consumed independently by both apps/api's EmailService and
                // apps/lighthaus-api's DispatcherService — each reads its own
                // env file, so both need the write.
                "EMAIL_DEV_REDIRECT" | "EMAIL_ENABLED" | "RESEND_API_KEY"
                | "EMAIL_FROM" | "OPS_RECIPIENTS" => vec![
                    ServiceTarget { label: "api", env_file: "/srv/sitehaus/apps/api/.env", container: "sitehaus-api-1", compose_file },
                    ServiceTarget { label: "lighthaus-api", env_file: "/srv/sitehaus/apps/lighthaus-api/.env", container: "sitehaus-lighthaus-api-1", compose_file },
                ],
                // Everything else (JWT_SECRET, ACCESS_TTL_SEC, DATABASE_URL,
                // COOKIE_DOMAIN, …) is api-only, matching rules_for's manifest.
                _ => vec![
                    ServiceTarget { label: "api", env_file: "/srv/sitehaus/apps/api/.env", container: "sitehaus-api-1", compose_file },
                ],
            }
        }
    }
}

// Ecom deploys the SAME compose file to both prod and staging (see
// ECOM_COMPOSE above), differentiated only by IMAGE_TAG — the CD pipeline's
// own deploy script sets this explicitly before every compose invocation.
// `docker compose up -d` re-resolves ${IMAGE_TAG:-latest}, so without it a
// staging restart silently pulls the PRODUCTION image (the compose file's
// fallback default is "latest").
fn restart_command(compose_file: &str, label: &str, server_type: &ServerType, is_prod: bool) -> String {
    let image_tag_prefix = if matches!(server_type, ServerType::Ecom) && !is_prod {
        "IMAGE_TAG=staging "
    } else {
        ""
    };
    format!("{image_tag_prefix}docker compose -f {compose_file} up -d {label}")
}

// ─── Advisory checks ──────────────────────────────────────────────────────────

fn check_no_localhost(value: &str, _is_prod: bool) -> Option<String> {
    if value.contains("localhost") || value.contains("127.0.0.1") {
        Some("points to localhost — is this right for a remote server?".to_string())
    } else {
        None
    }
}

fn check_secret_length(value: &str, _is_prod: bool) -> Option<String> {
    if value.len() < 32 {
        Some(format!("only {} chars — minimum 32 recommended", value.len()))
    } else {
        None
    }
}

fn check_dev_redirect_in_prod(_value: &str, is_prod: bool) -> Option<String> {
    if is_prod {
        Some("set on a production server — all emails will be redirected away from real recipients".to_string())
    } else {
        None
    }
}

fn check_stripe_key(value: &str, is_prod: bool) -> Option<String> {
    if is_prod && value.starts_with("sk_test_") {
        Some("test key in use on a production server".to_string())
    } else {
        None
    }
}

fn check_cors_enforce(value: &str, is_prod: bool) -> Option<String> {
    if is_prod && value != "true" {
        Some("CORS is in soak mode on a production server — disallowed origins are permitted, not rejected".to_string())
    } else {
        None
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn fetch_env(
    server: &crate::config::ServerConfig,
    container: &str,
) -> Result<HashMap<String, String>> {
    let raw = ssh_capture(server, &format!("docker exec {container} env"))?;
    let mut map = HashMap::new();
    for line in raw.lines() {
        if let Some((k, v)) = line.split_once('=') {
            map.insert(k.to_string(), v.to_string());
        }
    }
    Ok(map)
}

fn mask(key: &str, value: &str) -> String {
    const SECRET_KEYS: &[&str] = &[
        "DATABASE_URL",
        "JWT_SECRET",
        "SESSION_SECRET",
        "STRIPE_SECRET_KEY",
        "STRIPE_WEBHOOK_SECRET",
        "R2_SECRET_ACCESS_KEY",
        "RESEND_API_KEY",
    ];
    if SECRET_KEYS.contains(&key) {
        let visible = value.chars().take(4).collect::<String>();
        format!("{visible}{}", "*".repeat(8))
    } else {
        value.to_string()
    }
}

/// Escape a string for use inside bash single-quotes.
fn sh_escape(s: &str) -> String {
    s.replace('\'', "'\\''")
}

// ─── env check ────────────────────────────────────────────────────────────────

fn run_check(server_override: Option<&str>) -> Result<()> {
    let config = read_config()?;
    let (name, server) = resolve_server(&config, server_override)?;
    let is_prod = crate::confirm::is_prod(name);

    println!("\nChecking env vars on {}...\n", theme::yellow(name));

    let rules = rules_for(&server.server_type);

    // Collect unique containers and fetch each once
    let unique_containers: Vec<&str> = {
        let mut seen = std::collections::HashSet::new();
        rules.iter().filter_map(|r| {
            if seen.insert(r.container) { Some(r.container) } else { None }
        }).collect()
    };

    let mut envs: HashMap<&str, HashMap<String, String>> = HashMap::new();
    for container in unique_containers {
        let env = fetch_env(server, container).unwrap_or_default();
        envs.insert(container, env);
    }

    let tick  = "✓".green().bold().to_string();
    let cross = "✗".red().bold().to_string();
    let warn  = "⚠".yellow().bold().to_string();

    let mut missing  = 0usize;
    let mut warnings = 0usize;
    let empty_map    = HashMap::new();

    for rule in &rules {
        let env = envs.get(rule.container).unwrap_or(&empty_map);
        match env.get(rule.key) {
            None => {
                if rule.required {
                    println!("  {cross}  {:<28} {}", rule.key, "missing".red());
                    missing += 1;
                } else {
                    println!("  {}  {:<28} {}", "–".dimmed(), rule.key, "not set (optional)".dimmed());
                }
            }
            Some(value) => {
                if let Some(check_fn) = rule.check {
                    if let Some(msg) = check_fn(value, is_prod) {
                        println!("  {warn}  {:<28} {}", rule.key, msg.yellow());
                        warnings += 1;
                        continue;
                    }
                }
                println!("  {tick}  {:<28} {}", rule.key, mask(rule.key, value).dimmed());
            }
        }
    }

    println!();

    if missing > 0 {
        theme::error(&format!(
            "{missing} required var{} missing on \"{name}\".",
            if missing == 1 { " is" } else { "s are" }
        ));
    } else if warnings > 0 {
        println!(
            "  {} All required vars are set on \"{name}\" ({warnings} warning{}).",
            "⚠".yellow(),
            if warnings == 1 { "" } else { "s" }
        );
    } else {
        theme::success(&format!("All env vars look good on \"{name}\"."));
    }

    println!();
    Ok(())
}

// ─── env set ──────────────────────────────────────────────────────────────────

fn run_set(pair: &str, server_override: Option<&str>) -> Result<()> {
    let (key, value) = pair
        .split_once('=')
        .ok_or_else(|| anyhow::anyhow!("expected KEY=VALUE format, e.g. SESSION_SECRET=abc123"))?;

    let key   = key.trim();
    let value = value.trim();

    if key.is_empty() {
        anyhow::bail!("key cannot be empty");
    }

    let config = read_config()?;
    let (name, server) = resolve_server(&config, server_override)?;
    let is_prod = crate::confirm::is_prod(name);

    let targets = targets_for_key(key, &server.server_type, is_prod);
    let labels: Vec<&str> = targets.iter().map(|t| t.label).collect();

    confirm(&format!(
        "Set {} on \"{}\" ({})?",
        theme::yellow(key),
        theme::yellow(name),
        labels.join(", "),
    ))?;

    let ek = sh_escape(key);
    let ev = sh_escape(value);
    let mut restart_failures: Vec<&str> = Vec::new();

    for target in &targets {
        println!("\n  {} Writing to {}", "→".dimmed(), target.env_file.dimmed());

        // Update-or-append: strip any existing entry then append the new one
        let write_cmd = format!(
            "_K='{ek}' && _V='{ev}' && \
             FILE='{path}' && \
             mkdir -p \"$(dirname \"$FILE\")\" && \
             touch \"$FILE\" && \
             {{ grep -v \"^${{_K}}=\" \"$FILE\" > \"${{FILE}}.tmp\" 2>/dev/null || true; }} && \
             mv \"${{FILE}}.tmp\" \"$FILE\" && \
             printf '%s=%s\\n' \"$_K\" \"$_V\" >> \"$FILE\"",
            path = target.env_file,
        );

        let code = ssh_exec(server, &write_cmd);
        if code != 0 {
            anyhow::bail!("failed to write {} to {} on \"{}\"", key, target.env_file, name);
        }

        println!("  {} Restarting {}...", "→".dimmed(), target.label.dimmed());
        let restart_code = ssh_exec(
            server,
            &restart_command(target.compose_file, target.label, &server.server_type, is_prod),
        );
        if restart_code != 0 {
            theme::error(&format!(
                "restart of \"{}\" failed (exit {restart_code}) — the value is written to {}, \
                 but the running container has not picked it up",
                target.label, target.env_file,
            ));
            restart_failures.push(target.label);
        }
    }

    println!();
    if restart_failures.is_empty() {
        theme::success(&format!("{} set on \"{}\".", theme::yellow(key), name));
    } else {
        theme::error(&format!(
            "{} written on \"{}\", but {} did not restart — value is not live yet. \
             Investigate and restart manually: sitehaus restart {} --server {}",
            theme::yellow(key), name, restart_failures.join(", "), restart_failures.join(" "), name,
        ));
    }
    println!();
    Ok(())
}

// ─── Dispatch ─────────────────────────────────────────────────────────────────

pub fn run(cmd: &EnvCommand, server_override: Option<&str>) -> Result<()> {
    match cmd {
        EnvCommand::Check => run_check(server_override),
        EnvCommand::Set { pair } => run_set(pair, server_override),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_email_keys_target_both_api_and_lighthaus_api() {
        for key in ["EMAIL_DEV_REDIRECT", "EMAIL_ENABLED", "RESEND_API_KEY", "EMAIL_FROM", "OPS_RECIPIENTS"] {
            let targets = targets_for_key(key, &ServerType::Platform, true);
            let labels: Vec<&str> = targets.iter().map(|t| t.label).collect();
            assert_eq!(labels, vec!["api", "lighthaus-api"], "key {key} on prod");

            let targets = targets_for_key(key, &ServerType::Platform, false);
            let labels: Vec<&str> = targets.iter().map(|t| t.label).collect();
            assert_eq!(labels, vec!["api", "lighthaus-api"], "key {key} on staging");
        }
    }

    #[test]
    fn platform_other_keys_target_api_only() {
        let targets = targets_for_key("JWT_SECRET", &ServerType::Platform, true);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].label, "api");
    }

    #[test]
    fn platform_api_env_file_matches_what_both_compose_files_actually_declare() {
        // Both docker-compose.prod.yml and docker-compose.staging.yml declare
        // `api`'s env_file as ./apps/api/.env — never a bare root .env.
        for is_prod in [true, false] {
            let targets = targets_for_key("JWT_SECRET", &ServerType::Platform, is_prod);
            assert_eq!(targets[0].env_file, "/srv/sitehaus/apps/api/.env");
        }
    }

    #[test]
    fn platform_compose_file_is_environment_specific() {
        let prod = targets_for_key("JWT_SECRET", &ServerType::Platform, true);
        assert_eq!(prod[0].compose_file, "/srv/sitehaus/docker-compose.prod.yml");

        let staging = targets_for_key("JWT_SECRET", &ServerType::Platform, false);
        assert_eq!(staging[0].compose_file, "/srv/sitehaus/docker-compose.staging.yml");

        // The two must actually differ — this is the regression the bug shipped as:
        // both branches silently resolving to the same hardcoded file.
        assert_ne!(prod[0].compose_file, staging[0].compose_file);
    }

    #[test]
    fn ecom_compose_file_is_the_same_regardless_of_environment() {
        // sitehaus-commerce deploys ONE compose file to both commerce-prod and
        // commerce-staging — unlike Platform, there's no is_prod branch to get wrong.
        let prod = targets_for_key("STRIPE_SECRET_KEY", &ServerType::Ecom, true);
        let staging = targets_for_key("STRIPE_SECRET_KEY", &ServerType::Ecom, false);
        assert_eq!(prod[0].compose_file, staging[0].compose_file);
    }

    #[test]
    fn ecom_staging_restart_pins_the_staging_image_tag() {
        // Regression: `docker compose up -d` re-resolves ${IMAGE_TAG:-latest}.
        // Without an explicit export here, a staging restart silently pulled
        // the production image — this is exactly the bug that broke store
        // resolution on commerce-staging.
        let cmd = restart_command(ECOM_COMPOSE, "gateway", &ServerType::Ecom, false);
        assert!(
            cmd.starts_with("IMAGE_TAG=staging "),
            "expected staging restart to pin IMAGE_TAG, got: {cmd}"
        );
        assert!(cmd.contains(&format!("docker compose -f {ECOM_COMPOSE} up -d gateway")));
    }

    #[test]
    fn ecom_prod_restart_leaves_image_tag_unset() {
        // Prod has no IMAGE_TAG var at all, so the compose file's own fallback
        // (":latest") is what should apply — no prefix here.
        let cmd = restart_command(ECOM_COMPOSE, "gateway", &ServerType::Ecom, true);
        assert!(!cmd.contains("IMAGE_TAG"), "expected no IMAGE_TAG override on prod, got: {cmd}");
    }

    #[test]
    fn platform_restart_never_touches_image_tag() {
        // Platform's two compose files hardcode their tags directly (no
        // ${IMAGE_TAG} substitution at all), so this prefix is Ecom-only.
        for is_prod in [true, false] {
            let cmd = restart_command(PLATFORM_COMPOSE_PROD, "api", &ServerType::Platform, is_prod);
            assert!(!cmd.contains("IMAGE_TAG"), "is_prod={is_prod}: {cmd}");
        }
    }
}
