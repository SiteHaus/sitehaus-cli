# sitehaus

[![CI](https://github.com/SiteHaus/sitehaus-cli/actions/workflows/ci.yml/badge.svg)](https://github.com/SiteHaus/sitehaus-cli/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/rust-1.88-orange?style=flat&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Built on SiteHaus](https://img.shields.io/badge/built_on-SiteHaus-%239C59D1?style=flat)](https://sitehaus.io)
[![License: MIT](https://img.shields.io/badge/license-MIT-%23FCF434?style=flat)](./LICENSE)

Internal CLI for managing SiteHaus servers. Set an active server once, run commands everywhere.

```
sitehaus use commerce-staging
sitehaus db seed
sitehaus logs gateway
sitehaus deploy
```

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/SiteHaus/sitehaus-cli/main/install.sh | sh
```

Or manually:

```bash
git clone https://github.com/SiteHaus/sitehaus-cli
cd sitehaus-cli
cargo install --path .
```

## Setup

```bash
sitehaus setup
```

Walks you through registering your servers interactively. Config lives at `~/.sitehaus/config.yml`.

## Commands

### Context

| Command                 | Description                 |
| ----------------------- | --------------------------- |
| `sitehaus use <server>` | Set the active server       |
| `sitehaus status`       | Show active server + health |

### Server management

| Command                         | Description       |
| ------------------------------- | ----------------- |
| `sitehaus server add`           | Register a server |
| `sitehaus server list`          | List all servers  |
| `sitehaus server remove <name>` | Remove a server   |

### ecom

| Command                   | Description                        |
| ------------------------- | ---------------------------------- |
| `sitehaus db seed`        | Seed the database                  |
| `sitehaus db migrate`     | Run migrations                     |
| `sitehaus db studio`      | Open Drizzle Studio via SSH tunnel |
| `sitehaus logs [service]` | Tail service logs                  |
| `sitehaus health`         | Check health endpoint              |
| `sitehaus deploy`         | Pull + restart all services        |

Any command accepts `--server=<name>` to override the active server.

## Config

```yaml
active_server: commerce-staging
servers:
  commerce-prod:
    type: ecom
    host: 1.2.3.4
    ssh_user: deploy
    repo_path: /srv/sitehaus-commerce
    health_url: https://api.commerce.sitehaus.dev/health
  commerce-staging:
    type: ecom
    host: 5.6.7.8
    ssh_user: deploy
    repo_path: /srv/sitehaus-commerce
    health_url: https://api.staging.commerce.sitehaus.dev/health
```
