# Pipedash push relay

The push relay lets a self-hosted `pipedash-web` server send an iOS push notification when a pipeline run fails, even when the Pipedash app is closed. Apple only accepts pushes from a holder of the app's APNs key, so the relay is the one small service that holds that key. Servers never see it.

```
iOS app ── registers device token ──────────────▶ relay ──▶ Apple (APNs) ──▶ iPhone
pipedash-web ── "pipeline X failed" + relay id ──▶ relay
```

The project owner runs one relay for the App Store build. You only need this page if you publish your own build of the app (your own bundle id and APNs key), or if you want to run the relay yourself.

## What you need

- An Apple developer account.
- An App ID with the **Push Notifications** capability. The default bundle id is `com.henrique.pipedash`.
- An APNs authentication key (`.p8`).

### Create the APNs key

1. Open [Certificates, Identifiers & Profiles → Keys](https://developer.apple.com/account/resources/authkeys/list) in the Apple developer portal.
2. Click **+**, name the key, and tick **Apple Push Notifications service (APNs)**. Continue, then Register.
3. Copy the **Key ID** (10 characters). Download `AuthKey_<KEYID>.p8`. Apple lets you download it once.
4. Copy your **Team ID** from [Membership details](https://developer.apple.com/account#MembershipDetailsCard).

One key works for every app of the team, in both the sandbox and production APNs environments.

## Configuration

The relay reads these environment variables.

| Variable | Required | Default | Meaning |
| --- | --- | --- | --- |
| `RELAY_APNS_KEY` | one of the two | | The `.p8` file contents (PEM). `\n` escape sequences are accepted for single-line env files. |
| `RELAY_APNS_KEY_PATH` | one of the two | | Path to the `.p8` file. Used when `RELAY_APNS_KEY` is empty. |
| `RELAY_APNS_KEY_ID` | yes | | The 10-character Key ID. |
| `RELAY_APNS_TEAM_ID` | yes | | Your Apple Team ID. |
| `RELAY_ALLOWED_BUNDLE_IDS` | yes | | Comma-separated bundle ids the relay accepts, for example `com.henrique.pipedash`. It is also used as the `apns-topic`. |
| `RELAY_BIND_ADDR` | no | `0.0.0.0:8080` | Listen address. |
| `RELAY_DATABASE_PATH` | no | `pipedash-push-relay.db` (`/data/relay.db` in the image) | SQLite file. |
| `RELAY_TRUSTED_PROXY` | no | `false` | `true` or `1` to take the client address for registration limits from the last `X-Forwarded-For` entry. |
| `RUST_LOG` | no | `info` | Log filter. |

## Run it with Docker

Build the image from the repository root:

```sh
docker build -f docker/push-relay.Dockerfile -t pipedash-push-relay .
```

Run it:

```sh
docker run -d --name pipedash-push-relay \
  -p 8080:8080 \
  -v pipedash-push-relay-data:/data \
  -v "$PWD/AuthKey_ABC123DEFG.p8:/run/secrets/apns.p8:ro" \
  -e RELAY_APNS_KEY_PATH=/run/secrets/apns.p8 \
  -e RELAY_APNS_KEY_ID=ABC123DEFG \
  -e RELAY_APNS_TEAM_ID=6M376JWU73 \
  -e RELAY_ALLOWED_BUNDLE_IDS=com.henrique.pipedash \
  pipedash-push-relay
```

The container runs as user `65532`. The key file must be readable by that user, and a bind-mounted `/data` must be writable by it. Named volumes work as is.

The relay speaks plain HTTP. Put it behind a reverse proxy that terminates TLS, and use the `https://` URL everywhere else. `GET /healthz` returns `200 ok` for probes.

Do not enable request logging in the proxy for `/v1/` paths: the path of a subscription request contains its secret id.

## Point pipedash-web at the relay

Set the relay URL on every server that should send pushes. Either an environment variable:

```sh
PIPEDASH_PUSH_RELAY_URL=https://relay.example.com
```

or the config file:

```toml
[push]
relay_url = "https://relay.example.com"
```

The environment variable wins. Without a relay URL the server refuses to register devices (`422`) and sends nothing.

The iOS app is built with the same URL: set `PIPEDASH_PUSH_RELAY_URL` when you build it. That is the relay the app registers its device with. A server and an app must point at the same relay.

The iOS app keeps remote push and build watching off by default. Set `PipedashBuildWatchEnabled` to the plist boolean `true` in `crates/pipedash-desktop/Info.ios.plist` and rebuild to restore both features. See [the iOS flag instructions](../ios/README.md#local-build-watching).

Pairing happens in the iOS app. For each server you pair, it creates a separate relay subscription, then gives the server the resulting subscription id. Unpairing deletes that relay subscription. The server lists its devices at `GET /api/v1/push/subscriptions`, sends a test push with `POST /api/v1/push/subscriptions/{id}/test`, and forgets a device with `DELETE /api/v1/push/subscriptions/{id}`. All of these need the server's API token. The `id` in these paths is derived from the relay subscription id; the server never returns the relay subscription id itself.

A push is sent once for each run that newly moves into a failed state. A pipeline that was already failing when the server first saw it, a refresh, and a restart do not send anything again. If the relay answers `404` or `410` for a device, the server deletes that device.

## Relay API

| Method and path | Body | Answer |
| --- | --- | --- |
| `POST /v1/subscriptions` | `{ device_token, environment: "production" \| "sandbox", bundle_id }` | `201 { subscription_id }` |
| `PUT /v1/subscriptions/{subscription_id}` | `{ device_token }` | `204` |
| `DELETE /v1/subscriptions/{subscription_id}` | | `204` |
| `POST /v1/notifications` | `{ subscription_id, title, body, thread_id? }` | `202`, `404` unknown id, `410` APNs says the token is gone (the relay deletes it), `429` rate limited, `502` APNs refused |
| `GET /healthz` | | `200` |

The subscription id is 32 random bytes (base64url) and is the only credential: whoever holds it can push to that device through the relay. Requests are limited to 16 KiB, a title to 512 bytes, a body to 2048 bytes, and each subscription to 30 notifications per minute.

Because anyone can create a subscription, registration is limited too: at most 20 registrations per client address per hour (`429` after that), and at most 10 subscriptions per device token. The 11th registration of a token deletes the oldest subscription of that token. A client address is the connection's peer address, with IPv6 grouped by /64. Behind a reverse proxy, set `RELAY_TRUSTED_PROXY=true` and the relay uses the last `X-Forwarded-For` entry instead. Only enable it when every request really passes through your proxy, otherwise clients can spoof their address.

The relay talks to `api.push.apple.com` (production) or `api.sandbox.push.apple.com` (sandbox) over HTTP/2, with `apns-push-type: alert` and the bundle id as `apns-topic`. It signs an ES256 token with your key, reuses it, and renews it before it is an hour old. It deletes a subscription when APNs answers `410`, `BadDeviceToken` or `Unregistered`.

## Data

Stored by the relay, in the SQLite file, for each subscription:

- the SHA-256 hash of the subscription id (never the id itself),
- the APNs device token,
- the APNs environment (`production` or `sandbox`),
- the bundle id,
- the creation time,
- the time of the last activity (creation, token rotation or a successful push).

Kept only in memory: rate-limit counters, keyed by subscription hash for pushes and by client address for registrations, and the current APNs token. Counters are never written to disk. A sweep every 5 minutes drops each one once its window has ended (1 minute for pushes, 1 hour for registrations).

Never stored or logged: notification titles, bodies and thread ids, subscription ids, device tokens, pipeline names, and client IP addresses. The relay logs start-up, how many inactive subscriptions it removed, and the HTTP status APNs answered for a refused push, nothing else.

A subscription is deleted when the app unpairs it, when APNs reports its token as invalid (which happens after the app is uninstalled and the next push is attempted), or when it has been inactive for 90 days. Activity means creation, a token rotation (the app sends one on every launch), or a successful push. The relay checks for inactive subscriptions every hour. To remove everything, delete the SQLite file.

Each `pipedash-web` server stores, in its own database, one row per paired device: the relay subscription id, a device name the app chose, the platform, when it was added and when a push last succeeded. These rows are permanent configuration. They move with the rest of the configuration when you migrate storage to another data directory or database backend, and a factory reset deletes them.
