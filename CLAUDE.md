# CLAUDE.md

This file provides guidance to Claude Code when working with the `sitehaus-cli` repository.

## Overview

Internal Rust CLI for managing SiteHaus production and staging servers. Wraps SSH + Docker Compose operations behind a simple, context-aware interface. Register servers once, then run commands against the active server without re-specifying credentials.

## Technology Stack

- **Language**: Rust (edition 2024)
- **Arg parsing**: `clap` v4 with derive macros
- **HTTP**: `ureq` v2
- **Config**: `serde` + `serde_yaml` — stored at `~/.sitehaus/config.yml`
- **UX**: `dialoguer` (interactive prompts), `owo-colors` (colored output), `indicatif` (spinners), `tabled` (tables)

## Project Structure

```
src/
  main.rs           — CLI entry, command routing
  config.rs         — read/write ~/.sitehaus/config.yml
  commands/
    server.rs       — sitehaus server add/list/remove
    db.rs           — sitehaus db seed
    ops.rs          — sitehaus logs/health/deploy
    setup.rs        — sitehaus setup (first-run wizard)
  confirm.rs        — shared confirmation prompt helper
  ssh.rs            — SSH command execution helpers
  theme.rs          — gradient text, colors, success/error helpers
```

## Commands

```bash
# Build
cargo build

# Run locally (dev)
cargo run -- <command>

# Install globally
cargo install --path .
```

### CLI Commands

| Command | Description |
|---|---|
| `sitehaus setup` | Interactive first-run wizard — registers servers |
| `sitehaus use <name>` | Set the active server |
| `sitehaus status` | Show active server + health |
| `sitehaus server add` | Register a server |
| `sitehaus server list` | List registered servers |
| `sitehaus server remove <name>` | Remove a server |
| `sitehaus db seed` | Seed the database on active server |
| `sitehaus logs [service]` | Stream Docker Compose logs |
| `sitehaus health` | Check server health endpoint |
| `sitehaus deploy` | Pull latest images + restart all services |

### Log Services

Valid service names: `gateway`, `commerce`, `payments`, `worker`, `caddy`, `postgres`, `redis`

## Config Format

Stored at `~/.sitehaus/config.yml`:

```yaml
active_server: production
servers:
  - name: production
    host: your.server.ip
    ssh_user: deploy
    health_url: https://api.commerce.sitehaus.dev/health
  - name: staging
    host: staging.server.ip
    ssh_user: deploy
    health_url: https://api.staging.commerce.sitehaus.dev/health
```

## Key Patterns

- **Global `--server` flag**: Any command can override the active server with `--server <name>` without changing the stored active server.
- **SSH execution**: Commands run remotely via SSH. The `ssh.rs` module handles building and running SSH invocations.
- **No side effects on read**: `status` and `health` commands never modify config.
- **Theme consistency**: Always use `theme::success()`, `theme::error()`, `theme::yellow()` etc. — never raw `println!` with hardcoded ANSI codes.
