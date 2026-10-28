# Public demo

A read-only Pipedash on Postgres, connected to pre-seeded Gitea Actions, Jenkins, Komodo, and Argo CD, Tekton and Flux in one k3s container. Pipelines and deploys run on schedules, so runs, logs and metrics keep coming.

```sh
docker compose up -d
```

The first start takes about 10 minutes while the seed containers finish. Then open http://localhost:8080.

Visitors can browse everything. Demo mode (`PIPEDASH_DEMO_MODE=true`) rejects configuration changes and pipeline actions such as trigger, rerun and cancel.

To publish it, add the public hostname `demo.pipedash.app` → `http://localhost:8080` to an existing Cloudflare tunnel. Only Pipedash is exposed, and only on loopback.

The host needs about 8 GB of RAM. `docker compose down -v` deletes everything.

Until a release includes demo mode, build the image from the repository root and start with it:

```sh
docker build -f docker/Dockerfile -t pipedash-web:dev .
PIPEDASH_IMAGE=pipedash-web:dev docker compose up -d
```
