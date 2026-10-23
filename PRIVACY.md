# Privacy Policy for Pipedash

**Last updated:** October 22, 2026

## Overview

Pipedash is a CI/CD pipeline monitoring dashboard. We are committed to protecting your privacy.

## Data Collection

**Pipedash does not collect or store personal data on external servers, and never sends your credentials to anyone other than the CI/CD providers you configure.** When status-page checks are on, each request to a status-page host discloses ordinary HTTP metadata to that host: the IP address of the machine running Pipedash, a User-Agent with the Pipedash version, and the request times while the app is active.

All data, including:
- CI/CD provider credentials (API tokens, access keys)
- Pipeline configurations
- User preferences

...is stored **locally on your device only**.

## Third-Party Services

Pipedash connects to CI/CD providers (GitHub, GitLab, Jenkins, etc.) using credentials you provide. These connections are made directly from your device to those services. We do not proxy, intercept, or store any data from these connections.

To tell you when a provider has an outage, Pipedash also reads public status pages. These requests carry no credentials or cookies:
- `www.githubstatus.com` (GitHub, hosted by Atlassian Statuspage)
- `api.status.io` (GitLab.com, hosted by status.io)
- `bitbucket.status.atlassian.com` (Bitbucket, hosted by Atlassian Statuspage)
- `www.buildkitestatus.com` (Buildkite, hosted by Atlassian Statuspage)
- Any status page URL you configure on a provider

To turn this off, open Settings → General and switch off "Check provider status pages", or set `status_pages_enabled = false` in the `[general]` section of `config.toml`.

## Analytics

Pipedash does not include any analytics, tracking, or telemetry software.

## Data Security

Your credentials and data are stored securely on your device using platform-native secure storage mechanisms.

## Changes to This Policy

We may update this privacy policy from time to time. Any changes will be posted in this repository.

## Contact

If you have questions about this privacy policy, please open an issue at:
https://github.com/hcavarsan/pipedash/issues
