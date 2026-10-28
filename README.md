<div align="center">
  <br>
  <img src="./app-icon.png" width="102px" alt="Pipedash logo" />
  <h1>Pipedash</h1>
  <p>One dashboard for the CI/CD pipelines of every provider you use.</p>
  <a href="https://pipedash.app">Website</a> | <a href="https://pipedash.app/downloads">Downloads</a> | <a href="https://pipedash.app/docs">Docs</a>
  <br><br>
  <a href="https://github.com/hcavarsan/pipedash/releases">
    <img src="https://img.shields.io/github/v/release/hcavarsan/pipedash?style=for-the-badge" alt="Latest release">
  </a>
  <a href="https://github.com/hcavarsan/pipedash/releases">
    <img src="https://img.shields.io/github/downloads/hcavarsan/pipedash/total?style=for-the-badge" alt="Total downloads">
  </a>
  <a href="https://crates.io/crates/pipedash-web">
    <img src="https://img.shields.io/crates/v/pipedash-web?style=for-the-badge&logo=rust" alt="pipedash-web on crates.io">
  </a>
  <br><br>
  <a href="https://apps.apple.com/br/app/pipedash-ci-cd-monitoring/id6757285870?l=en-GB">
    <img src="https://developer.apple.com/app-store/marketing/guidelines/images/badge-download-on-the-app-store.svg" alt="Download on the App Store" height="40">
  </a>
  <br><br>
</div>

<p align="center">
  <a href="https://pipedash.app">
    <img src="https://pipedash.app/video/pipedash-hero-v5.webp" alt="Pipedash demo: the provider list, a workflow triggered with inputs, pinned workflows, a run history and build metrics" width="100%" />
  </a>
</p>

## About

Pipedash shows the pipelines of all your CI/CD providers in one table, so you stop switching between dashboards to find the build that failed. It polls each provider with the token or kubeconfig you add and caches the results. From the same table you open, trigger, rerun and cancel runs.

It runs as a desktop app, as a self-hosted web server in Docker or as an iOS app. All three share one Rust core.

```mermaid
flowchart LR
  subgraph Providers
    GH[GitHub Actions]
    GL[GitLab CI]
    K8S[Tekton, ArgoCD, FluxCD]
    More[Gitea, Bitbucket, Buildkite, Jenkins]
  end
  Providers -- API polling --> Core[pipedash-core]
  Core <--> DB[(SQLite or PostgreSQL)]
  Core --> Desktop[Desktop app]
  Core --> Web[Web server]
  Core --> iOS[iOS app]
```

## Supported providers

GitHub Actions, Gitea Actions, GitLab CI, Bitbucket Pipelines, Buildkite, Jenkins, Tekton CD, ArgoCD and FluxCD. Each one is a plugin crate. The [providers guide](https://pipedash.app/docs/providers) lists the token and permissions each provider needs.

## Features

- See the status of every pipeline, grouped by repository and workflow, and pin the ones you watch most. Hide a workflow you don't watch, and it leaves the table and its repository's status until you unhide it under Show hidden.
- Open the run history with commits, branches and durations, and track build metrics per pipeline.
- Read, search and download job logs of GitHub Actions and Buildkite runs without leaving Pipedash. Buildkite tokens need the `read_build_logs` scope.
- Open an ArgoCD sync to read its per-resource results, the app's recent events and the last 1000 lines of each pod container. A restarted container also shows its previous attempt. Pod logs need the `logs, get` permission in ArgoCD RBAC.
- Trigger a workflow with the inputs Pipedash loads from the provider, or rerun a past run with the same inputs.
- Cancel one run or several at once. On GitHub, approve deployment reviews and rerun only the failed jobs.
- Get a notification when a pipeline fails, a run takes far longer than usual or a provider has an outage.
- Keep provider tokens in the system keyring or in a database encrypted with AES-256-GCM. Pipedash sends no analytics or telemetry.

## Get started

Download the [desktop app](https://pipedash.app/downloads), install the [iOS app](https://apps.apple.com/br/app/pipedash-ci-cd-monitoring/id6757285870?l=en-GB) or run the [Docker image](https://pipedash.app/docs/deployment/docker). The [quick start](https://pipedash.app/docs/getting-started/quick-start) walks through your first provider.

- [Configuration](https://pipedash.app/docs/configuration): `config.toml`, environment variables and secrets.
- [Storage and tokens](https://pipedash.app/docs/configuration/storage-backends): SQLite, PostgreSQL and token encryption.
- [REST API](https://pipedash.app/docs/deployment/rest-api): routes, authentication and WebSocket events.
- [Architecture](https://pipedash.app/docs/resources/architecture): crates, refresh loop and plugin system.
- [Security](https://pipedash.app/docs/resources/security): encryption, API authentication and vulnerability reports.
- [Releases](https://github.com/hcavarsan/pipedash/releases): changes and release assets.

## Contributing

Read the [contribution guide](https://pipedash.app/docs/resources/contributing). For local builds, see [development](https://pipedash.app/docs/resources/development). To add a CI/CD provider, see [plugin development](https://pipedash.app/docs/resources/plugin-development).

Report bugs or request features through [GitHub issues](https://github.com/hcavarsan/pipedash/issues). For help, see [support](https://pipedash.app/support).

## License

Pipedash is licensed under [GPL-3.0](LICENSE).

## Star History

<a href="https://star-history.com/#hcavarsan/pipedash&Date">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=hcavarsan/pipedash&type=Date&theme=dark" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=hcavarsan/pipedash&type=Date" />
   <img alt="Star History Chart" src="https://api.star-history.com/svg?repos=hcavarsan/pipedash&type=Date" />
 </picture>
</a>

## Contributors

<a href="https://github.com/hcavarsan/pipedash/graphs/contributors">
  <img src="https://contrib.rocks/image?repo=hcavarsan/pipedash" alt="Pipedash contributors" />
</a>
