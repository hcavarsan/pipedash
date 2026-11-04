# Public demo

The stack behind [demo.pipedash.app](https://demo.pipedash.app): a read-only Pipedash connected to real Gitea Actions, Jenkins, Argo CD, Tekton, Flux and Komodo instances. Their pipelines run on schedules, fail now and then and post status page incidents, so there is always something to look at.

```sh
docker compose up -d
```

The first start takes about 10 minutes. Then open http://localhost:8080. It needs about 8 GB of RAM, and `docker compose down -v` removes everything.

Visitors sign in with the password `pipedash-demo`, shown on the sign-in screen, and can browse but not change anything: Pipedash holds read-only credentials and refuses every action and setting change.

To publish it, run it on its own VM and put this in `.env`:

```sh
DEMO_ADMIN_PASSWORD=<random>
DEMO_READER_PASSWORD=<random>
COMPOSE_PROFILES=public
TUNNEL_TOKEN=<Cloudflare tunnel token>
ANALYTICS_SNIPPET='<script defer src="https://analytics.example.com/script.js"></script>'
ANALYTICS_ORIGIN=https://analytics.example.com
PIPEDASH_PUSH_RELAY_URL=https://relay.pipedash.app
```

Route the tunnel's public hostname to `http://gateway:8080`. `ANALYTICS_SNIPPET` is optional and is added to every page. The gateway sends a strict Content Security Policy, so set `ANALYTICS_ORIGIN` to the origin the snippet loads from and reports to, or the browser blocks it.

`PIPEDASH_PUSH_RELAY_URL` is optional. It lets iPhones paired with the demo get push alerts, including the "Send test alert" button. Pipedash has no internet access in this stack, so `relay-egress` answers for `relay.pipedash.app` on the internal network and passes the TLS connection through to the real relay. It closes every connection for any other name, and it resolves the relay through `1.1.1.1` and `9.9.9.9`. Pipedash trusts the bundle that `status` writes to `/status-ca/bundle.crt`: the system roots plus the local status-page CA.

Until a release includes demo mode, build the image first:

```sh
docker build -f docker/Dockerfile -t pipedash-web:dev .
PIPEDASH_IMAGE=pipedash-web:dev docker compose up -d
```
