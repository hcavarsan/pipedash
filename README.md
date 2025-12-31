<div align="center">
  <br>
  <img src="./app-icon.png" width="102px" alt="Pipedash Logo" />
  <h1>Pipedash</h1>
  <p>A desktop app for managing CI/CD pipelines from multiple providers </p>

</div>

<p align="center">
<div align="center">
<img src="./public/pipedashbg.png" alt="Pipedash Screenshot" style="box-shadow: 0 10px 30px rgba(0, 0, 0, 0.3); border-radius: 8px;"/>
</div>
</p>

> **[WIP]** This is a work-in-progress project. It works for basic use cases but hasn't been thoroughly tested. Things might break, APIs might change, and there are probably bugs. Tested primarily on macOS – not sure if it works properly on Linux and Windows due to webview differences.

## About

Pipedash lets you see all your CI/CD pipelines in one place. Instead of jumping between GitHub Actions, GitLab CI, Buildkite, Jenkins, and Tekton dashboards, you get everything in a single view.

Built with Tauri, Rust, React, and TypeScript.

## Why

Most teams end up using multiple CI/CD platforms. Your open source projects run on GitHub Actions, internal services use GitLab CI or Buildkite, Kubernetes workloads use Tekton, and there's always that one Jenkins server handling legacy stuff. Checking everything means opening a bunch of tabs and hitting refresh.

Pipedash pulls your pipeline data from all these providers and shows it together.

## Supported providers

- GitHub Actions
- GitLab CI
- Bitbucket Pipelines
- Buildkite
- Jenkins
- Tekton CD
- ArgoCD

The plugin system makes it easy to add more.

## What it does

Pipedash polls your providers and shows pipelines organized by repo and workflow. It refreshes in the background (you can set the interval per provider). When a pipeline status changes, you'll see it immediately.

What you can do:
- See pipeline status across all your providers
- Browse run history with commit info and execution times
- Trigger workflows with parameters (Pipedash loads them from each provider)
- Re-run previous executions with the same parameters
- Cancel running builds
- Add multiple instances of the same provider (e.g., two GitHub orgs)

When you trigger or re-run a workflow, Pipedash fetches available parameters from the plugin (workflow inputs for GitHub Actions, pipeline variables for GitLab CI, build parameters for Jenkins/Buildkite) and shows them in a form.

**Privacy and security**

Everything runs locally on your machine. Pipedash only connects to your CI/CD providers – no analytics, telemetry, or third-party services.

Token storage options:

- **System keyring** (desktop) – Your OS encrypts tokens (macOS Keychain, Windows Credential Manager, Linux Secret Service). Default for desktop.
- **Encrypted SQLite** – Pipedash encrypts tokens with AES-256-GCM using Argon2id-derived keys. Works on all platforms.
- **Encrypted PostgreSQL** – Same AES-256-GCM encryption, stored in PostgreSQL. Use this with the PostgreSQL backend.

You can store pipeline data in SQLite or PostgreSQL on both desktop and web. Switch backends anytime via the setup wizard.

**API Authentication** (web deployments)

When `PIPEDASH_VAULT_PASSWORD` is set, Pipedash requires authentication for API requests. The vault password serves dual purpose: encrypting your provider tokens AND securing API access. All requests must include the header `Authorization: Bearer <vault_password>`. If the env var is not set, the API remains open (suitable for local use or behind a VPN).

## Deployment options

You can run Pipedash in three ways:

**Desktop app (Tauri)**

The main way to use Pipedash. It's a native desktop app for macOS, Windows, and Linux. By default, your tokens go in the system keyring. You can also use encrypted SQLite or PostgreSQL.

Download from the [releases page](https://github.com/hcavarsan/pipedash/releases).

**REST API server**

For server deployments, CI/CD integrations, or headless operation. Run `pipedash-web` as a standalone service.

```bash
# Config via environment variables
export PIPEDASH_BIND_ADDR=127.0.0.1:8080
export PIPEDASH_TOKEN_1=ghp_xxx  # Token for provider ID 1

cargo run -p pipedash-web
```

The API server gives you:
- REST endpoints at `/api/v1/providers`, `/api/v1/pipelines`, `/api/v1/plugins`, etc.
- WebSocket endpoint at `/api/v1/ws` for real-time updates
- Same features as the desktop app

**Docker deployment**

The `examples/` directory has ready-to-use setups with sample configs. Edit the compose file to add your tokens, then:

```bash
# SQLite backend (simplest)
mise run examples:sqlite:up
mise run examples:sqlite:down

# PostgreSQL backend
mise run examples:postgres:up
mise run examples:postgres:down
```

The API server serves the frontend directly (it's embedded in the binary). Pipedash encrypts your tokens with AES-256-GCM. Your data persists in a Docker volume. See [Docker setup](#docker-setup) for details.

## Installation

Grab the latest release for your platform from the [releases page](https://github.com/hcavarsan/pipedash/releases).

Works on macOS, Windows, and Linux.

## Setup

Launch the app and add a provider via the sidebar. Each provider needs an API token:

**GitHub Actions**: Personal Access Token with `repo` and `workflow` scopes. You can set a custom base URL for GitHub Enterprise.

**GitLab CI**: Personal Access Token with `api` scope. Works with GitLab.com and self-hosted.

**Buildkite**: API Access Token with read permissions and your org slug.

**Jenkins**: API token, username, and server URL.

**Tekton CD**: Kubernetes config file path and context. Pipedash auto-detects namespaces with Tekton pipelines.

**ArgoCD**: Server URL and auth token. You can filter by Git orgs. Pipedash monitors sync status, health, and deployment history.

**Bitbucket Pipelines**: App password with `repository` and `pipeline` read permissions. Works with Bitbucket Cloud and self-hosted.

After you add a provider, Pipedash validates your credentials and fetches available repos. Pick which ones to monitor and save. Your pipelines will show up in the main view and refresh automatically.

### Initial setup

On first launch (or via settings), a setup wizard walks you through config. Works on both desktop and web.

**Storage backend**
- **SQLite** (default) – Local file-based storage.
- **PostgreSQL** – Centralized database for team deployments.

**Token storage**
- **System keyring** (desktop only) – Your OS handles encryption. No password needed.
- **Encrypted SQLite** – Pipedash encrypts with AES-256-GCM + Argon2id key derivation.
- **Encrypted PostgreSQL** – Same encryption, stored in PostgreSQL (requires PostgreSQL backend).

**Data migration**

You can switch storage backends anytime. The wizard offers to migrate your existing data (providers, credentials, pipeline history) or start fresh.

## How it works

**Architecture**

The app is split into three crates:
- `pipedash-desktop` – Tauri desktop app with system keyring integration
- `pipedash-core` – Core library with all the business logic (framework-agnostic)
- `pipedash-web` – REST API server for headless deployments

This lets the same core code run in different contexts (desktop app, API server, or embedded in other apps).

**Storage**

Pipedash stores your provider configs and pipeline data in SQLite (default) or PostgreSQL. Both work on desktop and web.

Token storage options:
- **System keyring** (desktop) – macOS Keychain, Windows Credential Manager, Linux Secret Service
- **Encrypted SQLite** – AES-256-GCM encryption with Argon2id key derivation
- **Encrypted PostgreSQL** – AES-256-GCM encryption in `encrypted_tokens` table

For encrypted storage, Pipedash auto-generates a password on first run. Set `PIPEDASH_VAULT_PASSWORD` if you need reproducible deployments.

Each provider has its own refresh interval (default: 30 seconds). Adjust based on API rate limits.

**Plugin system**

Each CI/CD provider is a plugin with a common interface. The core app doesn't know the specifics of GitHub Actions, GitLab CI, Bitbucket Pipelines, Buildkite, Jenkins, Tekton, or ArgoCD—it just calls methods like `fetch_pipelines()` or `trigger_pipeline()` and the plugin handles the details.

We compile plugins into the app at build time (not loaded dynamically at runtime). This keeps things simpler and avoids security concerns with runtime plugin loading.

When you start the app, Pipedash loads cached pipeline data from SQLite immediately. In the background, a refresh loop polls each provider's API and updates the cache when it detects changes. The frontend listens for events and re-renders when new data arrives.

## Adding providers

Want to add a new CI/CD platform? Create a crate in `crates/pipedash-plugin-{name}/` and implement the `Plugin` trait from `pipedash-plugin-api`. The trait defines methods for fetching pipelines, validating credentials, triggering builds, and getting run history.

Each plugin follows this structure:
- `schema.rs` - Table and column definitions for the provider
- `metadata.rs` - Plugin metadata, config schema, and capabilities
- `plugin.rs` - Main plugin trait implementation
- `client.rs` - API client for the provider
- `mapper.rs` - Data transformation between provider API and domain models
- `types.rs` - Provider-specific types
- `config.rs` - Config parsing utilities

After you implement the plugin, register it in the main app's plugin registry and add any provider-specific UI in the frontend.

Check out the existing plugins (GitHub Actions, GitLab CI, Bitbucket Pipelines, Buildkite, Jenkins, Tekton CD, ArgoCD) as reference implementations.



## Development

We use [mise](https://mise.jdx.dev) for dev environment and task management.

```bash
curl https://mise.run | sh

git clone https://github.com/hcavarsan/pipedash.git
cd pipedash

mise install
mise run install

mise run dev
```

### Development workflow

**Initial setup** (first time only):

- Install OS-specific Tauri prerequisites (see https://tauri.app/start/prerequisites/#system-dependencies)
- `mise install` – installs dev tools: Rust nightly (with clippy, rustfmt), Node 24, Bun
- `mise run install` – installs project dependencies: npm packages including @tauri-apps/cli
- You're ready to develop
- `mise run dev` – starts the app with hot reload


**Available commands**:

```bash
# Development
mise run dev           # full-stack with hot reload (API + frontend)
mise run dev:front     # frontend only (Vite dev server)
mise run dev:back      # backend only (API server with cargo watch)
mise run dev:tauri     # desktop app development
mise run dev:ios       # iOS emulator
mise run dev:android   # Android emulator

# Build
mise run build         # production binary with embedded frontend
mise run build:front   # frontend assets only
mise run build:tauri   # desktop app
mise run build:ios     # iOS app
mise run build:android # Android app
mise run build:docker  # Docker image

# Code quality
mise run fmt           # format all code
mise run fmt:front     # format frontend (ESLint --fix)
mise run fmt:back      # format backend (cargo fmt)
mise run lint          # lint all code
mise run lint:front    # lint frontend (ESLint)
mise run lint:back     # lint backend (Clippy)
mise run check         # type check all
mise run check:front   # TypeScript check
mise run check:back    # Cargo check

# CI / Release
mise run ci            # run all CI checks (fmt, lint, check)
mise run precommit     # pre-commit hook (fmt + lint + check)
mise run release       # full release build (ci + build)

# Docker
mise run docker:up     # start Docker Compose stack
mise run docker:down   # stop Docker Compose stack
mise run docker:logs   # view Docker Compose logs

# Utilities
mise run clean         # remove build artifacts
mise run knip          # find unused code
```

## Docker setup

Run Pipedash as a web app via Docker Compose:

```bash
docker-compose up -d        # start
docker-compose logs -f      # view logs
docker-compose down         # stop
docker-compose down -v      # stop and reset data
```

Open http://localhost:8080 in your browser.

The API server serves the frontend directly (it's embedded in the binary). A single container runs both.

**Token storage**: Add providers via the web UI. Pipedash encrypts your tokens with AES-256-GCM and stores them in the database. The encryption key is auto-generated on first run and persists in the Docker volume.

**Custom port**: Set `PIPEDASH_PORT` in your environment or `.env` file:

```bash
PIPEDASH_PORT=3000 docker-compose up -d
```

## Config

**Environment variables**

| Variable | Default | Description |
|----------|---------|-------------|
| `PIPEDASH_DATA_DIR` | Platform default | Where Pipedash stores data files |
| `PIPEDASH_DB_PATH` | `$DATA_DIR/pipedash.db` | Main database path |
| `PIPEDASH_METRICS_DB_PATH` | `$DATA_DIR/metrics.db` | Metrics database path |
| `PIPEDASH_METRICS_ENABLED` | `true` | Turn metrics collection on/off |
| `PIPEDASH_BIND_ADDR` | `127.0.0.1:8080` | API server bind address |
| `PIPEDASH_VAULT_PASSWORD` | Auto-generated | Password for encrypted token storage and API authentication |
| `PIPEDASH_EMBEDDED_FRONTEND` | `true` | Serve frontend from API binary |
| `PIPEDASH_CONFIG_PATH` | Auto-discovered | Path to TOML configuration file |
| `PIPEDASH_POSTGRES_URL` | – | PostgreSQL connection string |
| `PIPEDASH_PORT` | `8080` | Docker host port (docker-compose only) |
| `RUST_LOG` | `info` | Log level (debug, info, warn, error) |

**Configuration file**

You can also configure Pipedash via TOML. Set `PIPEDASH_CONFIG_PATH` to specify the location, or Pipedash auto-discovers from platform-specific paths.

```toml
[general]
metrics_enabled = true
default_refresh_interval = 30

[server]
bind_addr = "0.0.0.0:8080"

[storage]
backend = "sqlite"  # or "postgres"

[storage.postgres]
connection_string = "${PIPEDASH_POSTGRES_URL}"

# Add providers with unique IDs
[providers.github-work]
name = "GitHub Work"
type = "github"
token = "${GITHUB_TOKEN}"
refresh_interval = 30

[providers.github-work.config]
base_url = "https://github.com"
selected_items = "org/repo1,org/repo2"

[providers.gitlab-internal]
name = "GitLab Internal"
type = "gitlab"
token = "${GITLAB_TOKEN}"

[providers.gitlab-internal.config]
base_url = "https://gitlab.company.com"
selected_items = "team/backend,team/frontend"
```

Use `${VAR}` syntax to reference environment variables in config values.

**Cargo features**

| Feature | What it does |
|---------|-------------|
| `postgres` | Adds PostgreSQL backend support (on by default) |
| `devtools` | Enables Tauri devtools (dev only) |
| `custom-protocol` | Uses Tauri custom protocol for production builds |
| `full` | Turns on all optional features |

## Roadmap

**Core features**
- [x] Local SQLite storage
- [x] PostgreSQL backend for centralized deployments
- [x] Encrypted token storage (keyring + AES-256-GCM)
- [x] Auto-refresh with configurable intervals
- [x] Multiple provider instances support
- [x] Build duration metrics
- [x] Customizable table columns per provider
- [x] Dynamic workflow parameters
- [x] Re-run with same parameters
- [x] Cancel running builds
- [x] REST API server mode
- [x] Web deployment via Docker
- [x] Setup wizard with data migration
- [ ] Advanced filtering and search
- [ ] Log viewing within the app (currently opens external links)
- [ ] Build artifacts download
- [ ] Build notifications
- [ ] Auto-updater for releases

**CI/CD providers**
- [x] GitHub Actions
- [x] GitLab CI
- [x] Bitbucket Pipelines
- [x] Buildkite
- [x] Jenkins
- [x] Tekton CD
- [x] ArgoCD
- [ ] CircleCI
- [ ] Flux CD
- [ ] Azure Pipelines
- [ ] AWS CodePipeline
- [ ] Google Cloud Build
- [ ] Travis CI
- [ ] Drone CI


**Platforms** (maybe)

- [ ] Android app
- [ ] iOS app (tested locally in simulator, needs polish for store release), current state:
<p align="center">
<div align="center">
<img src="./public/pipedashios.png" width="30%" alt="Pipedash ios Screenshot"/>
</div>
</p>



No timeline on these. They'll happen if there's need for them and time to implement.

## Contributing

Bug reports, feature requests, and PRs are welcome. If you need a specific CI/CD provider, open an issue or submit a plugin implementation.

No formal guidelines yet—the project's still taking shape.

## License

GPL 3.0 License. See [LICENSE](LICENSE) for details.

