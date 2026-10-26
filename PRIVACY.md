# Privacy Policy for Pipedash

**Last updated:** October 26, 2026

## Overview

Pipedash monitors CI/CD pipelines using the provider credentials and storage destinations you configure. It does not require a Pipedash account.

## Data Collection

Pipedash does not send analytics or telemetry to its developer. Provider requests send your credentials to the CI/CD service you select. Those services receive the request contents, your connection's IP address and ordinary HTTP metadata under their own privacy policies.

The iOS app uses a private SQLite database on the device and the iOS Keychain for provider tokens by default. Stored data includes provider addresses, repository selections, cached run details, metrics and preferences. Run details can contain names, commit messages, branch names and other information returned by your provider.

Local storage is the default, not a restriction on advanced configuration. If you configure PostgreSQL, Pipedash sends the selected database contents to that server. Password-encrypted token storage keeps credential values encrypted in SQLite or PostgreSQL. The database administrator controls that server's access, retention and backups. Storage migration copies data to the selected destination and can leave the source database behind.

Exports and files you share can leave the device through the destination or app you choose. Provider TOML exports replace provider tokens with variable references, but still contain provider configuration. Review exported files for sensitive addresses, repository names and other configuration before sharing them.

## Third-Party Services

Provider connections go directly from the app or your self-hosted Pipedash server to the CI/CD service unless you configure an HTTP proxy. The proxy operator can observe connection metadata. An unencrypted HTTP connection also exposes request contents to intermediaries. Pipedash's developer does not operate a provider-data proxy.

To tell you when a provider has an outage, Pipedash also reads public status pages. These requests carry no credentials or cookies:
- `www.githubstatus.com` (GitHub, hosted by Atlassian Statuspage)
- `api.status.io` (GitLab.com, hosted by status.io)
- `bitbucket.status.atlassian.com` (Bitbucket, hosted by Atlassian Statuspage)
- `www.buildkitestatus.com` (Buildkite, hosted by Atlassian Statuspage)
- Any status page URL you configure on a provider

Each status-page host receives your connection's IP address, a User-Agent containing the Pipedash version and request times while checks run. These hosts control their own request logs and retention.

To turn this off, open Settings → General and switch off "Check provider status pages", or set `status_pages_enabled = false` in the `[general]` section of `config.toml`.

## Retention and Deletion

- Provider settings and credentials remain in the configured storage until you remove the provider or complete a factory reset. In Settings > Providers, remove the provider to stop its refreshes and delete its saved credential. To invalidate a token everywhere, revoke it with the CI/CD provider too.
- Cached pipeline data stays in the configured cache until replacement, expiration or deletion. Settings > Cache lets you clear pipelines, run history, workflow parameters or all caches. Data can return on the next refresh while the provider remains configured.
- Metrics use the retention period in Settings > Metrics and any per-pipeline override. Cleanup runs while Pipedash runs, so expired records can remain while the app is closed. Disabling new metrics collection does not itself delete existing records.
- Factory reset removes providers and their TOML definitions, saved provider credentials, cached pipeline data and metrics from the active storage. It also clears app preferences and local app-lock settings. It keeps non-provider storage configuration and an encrypted vault's password, and does not revoke provider tokens remotely or erase separate databases and backups. Reset is not transactional: completed deletions remain after a later failure. Retry a failed reset before treating deletion as complete.
- Saved exports, shared copies, device backups and database backups have separate lifetimes. Delete exports through Files or their selected destination. Manage backups through your device or database administrator. Uninstalling the app does not revoke provider tokens or guarantee deletion of Keychain items. Pipedash does not promise forensic erasure of storage or backups.

To stop optional status-page requests, turn off "Check provider status pages" in Settings > General. Remove a provider to stop requests to its CI/CD service. Provider-side records, including workflows you started, follow that provider's retention and deletion controls.

## Previous iOS Push Pairings

Remote push notifications and Live Activities are disabled in the current iOS build. It does not offer pairing or register for remote notifications. Optional local notifications report changes detected while the app runs and refreshes.

If you paired an earlier build, disabling these features does not erase an existing server or relay record. The following describes those records and their retention. The push relay sits between your self-hosted Pipedash server and Apple's push service (APNs). Its source is in `crates/pipedash-push-relay`. Deployment details are in `docs/push-relay.md`.

When you pair a server, this is what the relay receives and stores:

| Data | Received from | Stored by the relay | For how long |
| --- | --- | --- | --- |
| APNs device token, APNs environment (sandbox or production), app bundle id | The iOS app, when you pair a server | Yes, with the time it was created and the time of its last activity (a token refresh by the app, or a successful push) | Until you unpair that server (the app deletes it), until Apple reports the token invalid (for example after you uninstall the app), or until 90 days pass with no activity. The app refreshes the token every time it launches. |
| A random subscription id, one per paired server | Generated by the relay, held by the app and the server | Only its SHA-256 hash | Same as the device token |
| Notification text (the pipeline name followed by "failed", and the provider name, branch and run number), and a thread id (the pipeline id) | Your Pipedash server, when a pipeline run fails | **No.** Held in memory while the relay passes it to Apple, then dropped | Not kept |
| The IP address of each request | The app and your server | **No.** The relay does not log it or store it. It uses the address of registration requests only for an in-memory counter that is dropped within about 65 minutes. A reverse proxy in front of the relay may log it. | Not kept by the relay |

The relay does not log device tokens, subscription ids or notification text. To limit abuse it keeps request counters in memory only: per subscription for pushes, and per client IP address (IPv6 grouped by /64) for new registrations. They are never written to disk. A counter is dropped by a sweep that runs every 5 minutes once its limit window has ended (1 minute for pushes, 1 hour for registrations), so an IP address is held for at most about 65 minutes. It does not receive your CI/CD credentials, API tokens, vault password or repository contents.

What your server stores for each paired device is kept in your own database: the relay subscription id, a device name chosen by the app, the platform, when it was added and when a push last succeeded. It moves with your configuration when you migrate storage, and a factory reset on the server deletes it.

Notification text passes through the relay and Apple's APNs in the clear over TLS. It is not end-to-end encrypted, so do not put anything in a pipeline name or branch name that you would not want Apple and the relay operator to be able to see. Apple's handling of push data is governed by Apple's own privacy policy.

To stop a previously paired server from sending alerts, remove its device subscription or remove `PIPEDASH_PUSH_RELAY_URL` (or the `[push]` section) from that server's configuration. Builds with pairing controls can unpair the server and request relay-record deletion. A factory reset attempts remote cleanup, but network or service failures can prevent it. Existing relay records otherwise follow the retention periods above. Contact the relay operator if you need deletion before expiry. Do not post device tokens, subscription identifiers or provider credentials in a public support issue.

## Analytics

Pipedash does not include any analytics, tracking, or telemetry software.

## Data Security

The default iOS setup stores provider tokens in the Keychain. Optional device authentication protects access through Pipedash. Password-encrypted credential storage uses AES-256-GCM with an Argon2id-derived key. This encrypts credential values, not the entire database or every configuration field. Protect your device, storage server, configuration files and exported copies accordingly.

## Changes to This Policy

We may update this privacy policy from time to time. Any changes will be posted in this repository.

## Contact

If you have questions about this privacy policy, please open an issue at:
https://github.com/hcavarsan/pipedash/issues
