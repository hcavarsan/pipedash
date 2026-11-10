<div align="center">
  <br>
  <img src="./app-icon.png" width="102px" alt="Pipedash logo" />
  <h3>Pipedash</h3>
  <p>One dashboard for the CI/CD pipelines of every provider you use.</p>
  <a href="https://pipedash.app">Website</a> | <a href="https://demo.pipedash.app">Live Demo</a> | <a href="https://pipedash.app/downloads">Downloads</a> | <a href="https://pipedash.app/docs">Docs</a>
  <br><br>
  <a href="https://trendshift.io/repositories/17107?utm_source=trendshift-badge&amp;utm_medium=badge&amp;utm_campaign=badge-trendshift-17107" target="_blank" rel="noopener noreferrer"><img src="https://trendshift.io/api/badge/trendshift/repositories/17107/daily?language=Rust" alt="hcavarsan%2Fpipedash | Trendshift" width="250" height="55"/></a>
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
  <a href="https://codecov.io/gh/hcavarsan/pipedash">
    <img src="https://img.shields.io/codecov/c/github/hcavarsan/pipedash/main?style=for-the-badge&logo=codecov" alt="Codecov coverage">
  </a>
  <br>
  <a href="https://apps.apple.com/br/app/pipedash-ci-cd-monitoring/id6757285870?l=en-GB">
    <img src="https://img.shields.io/badge/iOS-App%20Store-0D96F6?style=for-the-badge&logo=apple&logoColor=white" alt="iOS app on the App Store" height="40">
  </a>
  <a href="https://pipedash.app/android">
    <img src="https://img.shields.io/badge/Android-closed%20beta-3DDC84?style=for-the-badge&logo=android&logoColor=white" alt="Android closed beta" height="40">
  </a>
  <br><br>
</div>

<p align="center">
  <img src="./.github/diagrams/how-it-works.png" alt="How Pipedash works: it reads status and logs from the CI/CD providers, sends run and cancel actions back to them, and shows everything on desktop, web and mobile" width="100%" />
</p>

## About

Pipedash shows the pipelines of all your CI/CD providers in one table, so you stop switching between dashboards to find the build that failed. It polls each provider with the token or kubeconfig you add and caches the results. From the same table you open, trigger, rerun and cancel runs.

It runs as a desktop app, as a self-hosted web server in Docker or as an iOS and Android app. All of them share one Rust core.

### Supported providers

<div align="center">
<table>
  <tr>
    <td align="center" valign="top" width="150"><a href="https://pipedash.app/docs/providers/github-actions"><img src="./crates/pipedash-plugin-github/assets/icon.svg" width="40" height="40" alt="" /><br><b>GitHub&nbsp;Actions</b></a></td>
    <td align="center" valign="top" width="150"><a href="https://pipedash.app/docs/providers/gitlab-ci"><img src="./crates/pipedash-plugin-gitlab/assets/icon.svg" width="40" height="40" alt="" /><br><b>GitLab&nbsp;CI</b></a></td>
    <td align="center" valign="top" width="150"><a href="https://pipedash.app/docs/providers/gitea-actions"><img src="./crates/pipedash-plugin-gitea/assets/icon.svg" width="40" height="40" alt="" /><br><b>Gitea&nbsp;Actions</b></a></td>
    <td align="center" valign="top" width="150"><a href="https://pipedash.app/docs/providers/bitbucket-pipelines"><img src="./crates/pipedash-plugin-bitbucket/assets/icon.svg" width="40" height="40" alt="" /><br><b>Bitbucket&nbsp;Pipelines</b></a></td>
    <td align="center" valign="top" width="150"><a href="https://pipedash.app/docs/providers/azure-devops"><img src="./crates/pipedash-plugin-azure-devops/assets/icon.svg" width="40" height="40" alt="" /><br><b>Azure&nbsp;DevOps</b></a></td>
  </tr>
  <tr>
    <td align="center" valign="top" width="150"><a href="https://pipedash.app/docs/providers/circleci"><img src="./crates/pipedash-plugin-circleci/assets/icon.svg" width="40" height="40" alt="" /><br><b>CircleCI</b></a></td>
    <td align="center" valign="top" width="150"><a href="https://pipedash.app/docs/providers/buildkite"><img src="./crates/pipedash-plugin-buildkite/assets/icon.svg" width="40" height="40" alt="" /><br><b>Buildkite</b></a></td>
    <td align="center" valign="top" width="150"><a href="https://pipedash.app/docs/providers/jenkins"><img src="./crates/pipedash-plugin-jenkins/assets/icon.svg" width="40" height="40" alt="" /><br><b>Jenkins</b></a></td>
    <td align="center" valign="top" width="150"><a href="https://pipedash.app/docs/providers/woodpecker"><img src="./crates/pipedash-plugin-woodpecker/assets/icon.svg" width="40" height="40" alt="" /><br><b>Woodpecker&nbsp;CI</b></a></td>
    <td align="center" valign="top" width="150"><a href="https://pipedash.app/docs/providers/tekton"><img src="./crates/pipedash-plugin-tekton/assets/icon.svg" width="40" height="40" alt="" /><br><b>Tekton&nbsp;CD</b></a></td>
  </tr>
  <tr>
    <td align="center" valign="top" width="150"><a href="https://pipedash.app/docs/providers/argocd"><img src="./crates/pipedash-plugin-argocd/assets/icon.svg" width="40" height="40" alt="" /><br><b>ArgoCD</b></a></td>
    <td align="center" valign="top" width="150"><a href="https://pipedash.app/docs/providers/fluxcd"><img src="./crates/pipedash-plugin-fluxcd/assets/icon.svg" width="40" height="40" alt="" /><br><b>FluxCD</b></a></td>
    <td align="center" valign="top" width="150"><a href="https://pipedash.app/docs/providers/prow"><img src="./crates/pipedash-plugin-prow/assets/icon.svg" width="40" height="40" alt="" /><br><b>Prow</b></a></td>
    <td align="center" valign="top" width="150"><a href="https://pipedash.app/docs/providers/komodo"><img src="./crates/pipedash-plugin-komodo/assets/icon.svg" width="40" height="40" alt="" /><br><b>Komodo</b></a></td>
    <td align="center" valign="top" width="150"><a href="https://pipedash.app/docs/providers/coolify"><img src="./crates/pipedash-plugin-coolify/assets/icon.svg" width="40" height="40" alt="" /><br><b>Coolify</b></a></td>
  </tr>
</table>
</div>

## Features

<div align="center">
<table>
  <tr>
    <td align="center" width="120"><img src="./.github/icons/monitor.svg" width="32" height="32" alt="" /><br><b>Monitor</b></td>
    <td width="420">See the status of every pipeline in one place.</td>
    <td width="264"><a href="https://pipedash.app/docs/features/dashboard">Dashboard</a><br><a href="https://pipedash.app/docs/features/provider-health">Provider health</a><br><a href="https://pipedash.app/docs/features/refresh">Refresh</a></td>
  </tr>
  <tr>
    <td align="center" width="120"><img src="./.github/icons/organize.svg" width="32" height="32" alt="" /><br><b>Organize</b></td>
    <td width="420">Group pipelines into saved views with rules, pins and bulk actions.</td>
    <td width="264"><a href="https://pipedash.app/docs/features/views">Views</a></td>
  </tr>
  <tr>
    <td align="center" width="120"><img src="./.github/icons/investigate.svg" width="32" height="32" alt="" /><br><b>Investigate</b></td>
    <td width="420">Find out why a run failed.</td>
    <td width="264"><a href="https://pipedash.app/docs/features/run-logs">Run logs</a><br><a href="https://pipedash.app/docs/features/metrics">Metrics</a></td>
  </tr>
  <tr>
    <td align="center" width="120"><img src="./.github/icons/act.svg" width="32" height="32" alt="" /><br><b>Act</b></td>
    <td width="420">Start a workflow with inputs or stop a run.</td>
    <td width="264"><a href="https://pipedash.app/docs/features/trigger-and-cancel">Trigger and cancel</a></td>
  </tr>
  <tr>
    <td align="center" width="120"><img src="./.github/icons/notify.svg" width="32" height="32" alt="" /><br><b>Notify</b></td>
    <td width="420">Get an alert when a pipeline fails or recovers.</td>
    <td width="264"><a href="https://pipedash.app/docs/features/notifications">Notifications</a></td>
  </tr>
</table>
</div>

## Get started

- **Install** it with the [installation guide](https://pipedash.app/docs/getting-started/installation).
- **Add a provider** with the [quick start](https://pipedash.app/docs/getting-started/quick-start).
- **Run** it on [desktop](https://pipedash.app/docs/deployment/desktop), [mobile](https://pipedash.app/docs/deployment/mobile) or [Docker](https://pipedash.app/docs/deployment/docker).
- **Share** one server through [Server mode](https://pipedash.app/docs/deployment/server-mode), a [reverse proxy](https://pipedash.app/docs/deployment/reverse-proxy) or the [REST API](https://pipedash.app/docs/deployment/rest-api).
- **Configure** [settings](https://pipedash.app/docs/configuration), [storage](https://pipedash.app/docs/configuration/storage-backends), [tokens](https://pipedash.app/docs/configuration/token-storage) and the [cache](https://pipedash.app/docs/features/cache).

## Screenshots

<details>
<summary><b>Desktop and web</b> (9 pages)</summary>

<table>
  <tr>
    <td align="center" width="33%"><b>Pipelines</b><br><img src="https://pipedash.app/img/readme/desktop/01-pipelines.webp" alt="Desktop: Pipelines" width="100%" /></td>
    <td align="center" width="33%"><b>Run history</b><br><img src="https://pipedash.app/img/readme/desktop/02-run-history.webp" alt="Desktop: Run history" width="100%" /></td>
    <td align="center" width="33%"><b>Run details</b><br><img src="https://pipedash.app/img/readme/desktop/03-run-details.webp" alt="Desktop: Run details" width="100%" /></td>
  </tr>
  <tr>
    <td align="center" width="33%"><b>Job logs</b><br><img src="https://pipedash.app/img/readme/desktop/04-job-logs.webp" alt="Desktop: Job logs" width="100%" /></td>
    <td align="center" width="33%"><b>Trigger a workflow</b><br><img src="https://pipedash.app/img/readme/desktop/05-trigger-workflow.webp" alt="Desktop: Trigger a workflow" width="100%" /></td>
    <td align="center" width="33%"><b>Metrics</b><br><img src="https://pipedash.app/img/readme/desktop/06-metrics.webp" alt="Desktop: Metrics" width="100%" /></td>
  </tr>
  <tr>
    <td align="center" width="33%"><b>Provider status</b><br><img src="https://pipedash.app/img/readme/desktop/07-provider-status.webp" alt="Desktop: Provider status" width="100%" /></td>
    <td align="center" width="33%"><b>Notifications</b><br><img src="https://pipedash.app/img/readme/desktop/08-notifications.webp" alt="Desktop: Notifications" width="100%" /></td>
    <td align="center" width="33%"><b>Providers</b><br><img src="https://pipedash.app/img/readme/desktop/09-providers.webp" alt="Desktop: Providers" width="100%" /></td>
  </tr>
</table>

</details>

<details>
<summary><b>Mobile</b> (9 pages)</summary>

<table>
  <tr>
    <td align="center" width="33%"><b>Pipelines</b><br><img src="https://pipedash.app/img/readme/mobile/01-pipelines.webp" alt="Mobile: Pipelines" width="100%" /></td>
    <td align="center" width="33%"><b>Run history</b><br><img src="https://pipedash.app/img/readme/mobile/02-run-history.webp" alt="Mobile: Run history" width="100%" /></td>
    <td align="center" width="33%"><b>Run details</b><br><img src="https://pipedash.app/img/readme/mobile/03-run-details.webp" alt="Mobile: Run details" width="100%" /></td>
  </tr>
  <tr>
    <td align="center" width="33%"><b>Job logs</b><br><img src="https://pipedash.app/img/readme/mobile/04-job-logs.webp" alt="Mobile: Job logs" width="100%" /></td>
    <td align="center" width="33%"><b>Trigger a workflow</b><br><img src="https://pipedash.app/img/readme/mobile/05-trigger-workflow.webp" alt="Mobile: Trigger a workflow" width="100%" /></td>
    <td align="center" width="33%"><b>Metrics</b><br><img src="https://pipedash.app/img/readme/mobile/06-metrics.webp" alt="Mobile: Metrics" width="100%" /></td>
  </tr>
  <tr>
    <td align="center" width="33%"><b>Provider status</b><br><img src="https://pipedash.app/img/readme/mobile/07-provider-status.webp" alt="Mobile: Provider status" width="100%" /></td>
    <td align="center" width="33%"><b>Notifications</b><br><img src="https://pipedash.app/img/readme/mobile/08-notifications.webp" alt="Mobile: Notifications" width="100%" /></td>
    <td align="center" width="33%"><b>Providers</b><br><img src="https://pipedash.app/img/readme/mobile/09-providers.webp" alt="Mobile: Providers" width="100%" /></td>
  </tr>
</table>

</details>

## Contributing

- **Contribute** through the [contribution guide](https://pipedash.app/docs/resources/contributing) and [GitHub issues](https://github.com/hcavarsan/pipedash/issues).
- **Build** locally with the [development guide](https://pipedash.app/docs/resources/development). Run `mise install` first.
- **Write a plugin** for a new provider with [plugin development](https://pipedash.app/docs/resources/plugin-development).
- **Learn** the [architecture](https://pipedash.app/docs/resources/architecture) and the [security model](https://pipedash.app/docs/resources/security).
- **Get help** from [support](https://pipedash.app/support).

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
