# FluxCD validation environment

These fixtures run FluxCD in kind with healthy, failed and suspended Kustomizations and HelmReleases. They use podinfo Git tag `6.7.1` and chart `6.7.1`.

## Cluster

```sh
DOCKER_CONTEXT=<ctx> KUBECONFIG=~/.kube/pipedash-fluxcd.kubeconfig \
  kind create cluster --config examples/fluxcd/kind-cluster.yaml
chmod 600 ~/.kube/pipedash-fluxcd.kubeconfig
```

The API server listens on `127.0.0.1:46443` of the Docker host only. For a remote Docker host, forward it:

```sh
ssh -N -L 127.0.0.1:46443:127.0.0.1:46443 <docker-host>
```

## Flux and fixtures

```sh
export KUBECONFIG=~/.kube/pipedash-fluxcd.kubeconfig
kubectl apply -f https://github.com/fluxcd/flux2/releases/download/v2.9.6/install.yaml
kubectl -n flux-system wait --for=condition=Available deploy --all --timeout=300s
kubectl apply -k examples/fluxcd
```

Objects in namespace `pipedash-examples`:

| Kind | Name | Expected state |
| --- | --- | --- |
| Kustomization | `podinfo-healthy` | Ready |
| Kustomization | `podinfo-failed` | Not ready, path not found |
| Kustomization | `podinfo-suspended` | Suspended |
| HelmRelease | `podinfo-healthy` | Ready |
| HelmRelease | `podinfo-failed` | Not ready, install failed |
| HelmRelease | `podinfo-suspended` | Suspended |

`rbac.yaml` adds the `pipedash-readonly` service account, limited to `get` and `list` on these two kinds in `pipedash-examples`. It has no token secret. Create a short-lived token and kubeconfig user when needed:

```sh
kubectl -n pipedash-examples create token pipedash-readonly --duration 1h
```

## Connect Pipedash

1. Select "FluxCD" in Add provider.
2. Set Kubeconfig to the absolute path of `~/.kube/pipedash-fluxcd.kubeconfig` on the machine running Pipedash.
3. Select context `kind-pipedash-fluxcd` and enter namespace `pipedash-examples`.
4. Check permissions, continue to resource selection and select the 6 examples.

The resource list shows Ready, Not ready and Suspended states. Open a resource to see controller messages and native reconciliation history. "Reconcile" requests the current desired state without restoring an older revision or resuming a suspended resource.

Flux retains limited history. Pipedash preserves observed native entries in its cache and labels entries that Flux no longer retains. Clearing the cache removes them. Current-state snapshots are not executions and do not contribute to metrics.

History sorts by the latest observation, including groups reconciled again after newer groups. Metrics update those groups without adding duplicate executions. Missing selected resources appear as "Not found" without stopping the other resources from refreshing.

The failed Kustomization uses a missing path. The failed HelmRelease uses a nonexistent image tag and a 30-second timeout. Its controller message may describe an installation timeout rather than the image pull failure.

## Check

```sh
kubectl -n pipedash-examples get kustomizations,helmreleases
kubectl -n pipedash-examples annotate kustomization/podinfo-healthy --overwrite \
  reconcile.fluxcd.io/requestedAt="$(date +%s)"
```

## Standalone library checks

The consuming application chooses the Kubernetes API model. For standalone verification, use the workspace model:

```sh
K8S_OPENAPI_ENABLED_VERSION=1.32 cargo check -p pipedash-plugin-fluxcd --lib
K8S_OPENAPI_ENABLED_VERSION=1.32 cargo package -p pipedash-plugin-api -p pipedash-plugin-fluxcd
```

Package both crates together so verification uses the API from this checkout rather than its previously published version.
