use crate::config::{read_config, resolve_server, ServerType};
use crate::ssh::ssh_capture;
use crate::theme;
use anyhow::Result;
use owo_colors::OwoColorize;
use std::collections::HashMap;

struct EnvRule {
    key: &'static str,
    /// If true, `sitehaus env check` fails when this key is absent
    required: bool,
    /// Optional validation function — returns an advisory warning string or None
    check: Option<fn(&str, is_prod: bool) -> Option<String>>,
}

fn rules_for(server_type: &ServerType) -> Vec<EnvRule> {
    match server_type {
        ServerType::Ecom => vec![
            EnvRule { key: "DATABASE_URL",         required: true,  check: Some(check_no_localhost) },
            EnvRule { key: "REDIS_URL",             required: true,  check: Some(check_no_localhost) },
            EnvRule { key: "IAM_URL",               required: true,  check: Some(check_no_localhost) },
            EnvRule { key: "IAM_CLIENT_KEY",        required: true,  check: None },
            EnvRule { key: "SESSION_SECRET",        required: true,  check: Some(check_secret_length) },
            EnvRule { key: "STRIPE_SECRET_KEY",     required: false, check: Some(check_stripe_key) },
            EnvRule { key: "STRIPE_WEBHOOK_SECRET", required: false, check: None },
            EnvRule { key: "R2_ACCESS_KEY_ID",      required: false, check: None },
            EnvRule { key: "R2_SECRET_ACCESS_KEY",  required: false, check: None },
            EnvRule { key: "R2_BUCKET",             required: false, check: None },
            EnvRule { key: "R2_ENDPOINT",           required: false, check: None },
            EnvRule { key: "CDN_BASE_URL",          required: false, check: None },
            EnvRule { key: "PORT",                  required: false, check: None },
        ],
        ServerType::Platform => vec![
            EnvRule { key: "DATABASE_URL",     required: true,  check: Some(check_no_localhost) },
            EnvRule { key: "JWT_SECRET",       required: true,  check: Some(check_secret_length) },
            EnvRule { key: "ACCESS_TTL_SEC",   required: true,  check: None },
            EnvRule { key: "REFRESH_TTL_SEC",  required: true,  check: None },
            EnvRule { key: "RESEND_API_KEY",   required: false, check: None },
            EnvRule { key: "COOKIE_DOMAIN",    required: false, check: Some(check_no_localhost) },
            EnvRule { key: "COOKIE_SAME_SITE", required: false, check: None },
        ],
    }
}

/// Warn if a URL value contains "localhost" or "127.0.0.1"
fn check_no_localhost(value: &str, _is_prod: bool) -> Option<String> {
    if value.contains("localhost") || value.contains("127.0.0.1") {
        Some("points to localhost — is this right for a remote server?".to_string())
    } else {
        None
    }
}

/// Warn if a secret is shorter than 32 characters
fn check_secret_length(value: &str, _is_prod: bool) -> Option<String> {
    if value.len() < 32 {
        Some(format!("only {} chars — minimum 32 recommended", value.len()))
    } else {
        None
    }
}

/// Warn if a Stripe key looks like a test key on a prod server
fn check_stripe_key(value: &str, is_prod: bool) -> Option<String> {
    if is_prod && value.starts_with("sk_test_") {
        Some("test key in use on a production server".to_string())
    } else {
        None
    }
}

/// Fetch all env vars from the primary app container as a HashMap
fn fetch_env(
    server: &crate::config::ServerConfig,
    container: &str,
) -> Result<HashMap<String, String>> {
    // `docker exec <container> env` prints KEY=VALUE lines
    let raw = ssh_capture(server, &format!("docker exec {container} env"))?;
    let mut map = HashMap::new();
    for line in raw.lines() {
        if let Some((k, v)) = line.split_once('=') {
            map.insert(k.to_string(), v.to_string());
        }
    }
    Ok(map)
}

pub fn run(server_override: Option<&str>) -> Result<()> {
    let config = read_config()?;
    let (name, server) = resolve_server(&config, server_override)?;

    let is_prod = crate::confirm::is_prod(name);

    let container = match server.server_type {
        ServerType::Ecom => "sitehaus-commerce-commerce-1",
        ServerType::Platform => "sitehaus-api-1",
    };

    println!("\nChecking env vars on {}...\n", theme::yellow(name));

    let env = fetch_env(server, container)?;
    if env.is_empty() {
        anyhow::bail!(
            "could not read env from container \"{container}\" — is it running?"
        );
    }

    let rules = rules_for(&server.server_type);

    let tick  = "✓".green().bold().to_string();
    let cross = "✗".red().bold().to_string();
    let warn  = "⚠".yellow().bold().to_string();

    let mut missing = 0usize;
    let mut warnings = 0usize;

    for rule in &rules {
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
                // Run the advisory check if one exists
                if let Some(check_fn) = rule.check {
                    if let Some(msg) = check_fn(value, is_prod) {
                        println!("  {warn}  {:<28} {}", rule.key, msg.yellow());
                        warnings += 1;
                        continue;
                    }
                }
                // Mask secrets — show first 4 chars then asterisks
                let display = mask(rule.key, value);
                println!("  {tick}  {:<28} {}", rule.key, display.dimmed());
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

/// Mask sensitive values: show first 4 chars + asterisks, full value for non-secrets
fn mask(key: &str, value: &str) -> String {
    const SECRET_KEYS: &[&str] = &[
        "DATABASE_URL", "JWT_SECRET", "SESSION_SECRET",
        "STRIPE_SECRET_KEY", "STRIPE_WEBHOOK_SECRET",
        "R2_SECRET_ACCESS_KEY", "RESEND_API_KEY",
    ];
    if SECRET_KEYS.contains(&key) {
        let visible = value.chars().take(4).collect::<String>();
        format!("{visible}{}", "*".repeat(8))
    } else {
        value.to_string()
    }
}
