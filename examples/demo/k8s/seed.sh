#!/bin/sh
set -eu

until sed 's#https://127.0.0.1:6443#https://k3s:6443#' /shared/k3s.yaml > /shared/kubeconfig 2> /dev/null &&
  kubectl get nodes > /dev/null 2>&1; do
  sleep 2
done

gitea_ip=$(getent hosts gitea | awk '{ print $1 }')
kubectl create configmap coredns-custom -n kube-system --dry-run=client -o yaml \
  --from-literal=gitea.server="$(printf 'gitea:53 {\n  hosts {\n    %s gitea\n  }\n}\n' "$gitea_ip")" |
  kubectl apply -f -
until kubectl get deployment coredns -n kube-system > /dev/null 2>&1; do sleep 2; done
kubectl rollout restart deployment/coredns -n kube-system
kubectl rollout status deployment/coredns -n kube-system --timeout=180s

kubectl apply -f "https://infra.tekton.dev/tekton-releases/pipeline/previous/$TEKTON_VERSION/release.yaml"
kubectl apply -f "https://github.com/fluxcd/flux2/releases/download/$FLUX_VERSION/install.yaml"

helm repo add argo https://argoproj.github.io/argo-helm --force-update
helm upgrade --install argocd argo/argo-cd --version "$ARGOCD_CHART_VERSION" \
  -n argocd --create-namespace -f /seed/argocd-values.yaml --wait --timeout 10m

kubectl wait deployment --all -n tekton-pipelines --for=condition=Available --timeout=600s
kubectl wait deployment --all -n flux-system --for=condition=Available --timeout=600s

attempt=0
until kubectl apply -f /seed/manifests/; do
  attempt=$((attempt + 1))
  [ "$attempt" -lt 30 ] || exit 1
  sleep 10
done

if [ ! -s /credentials/argocd-token ]; then
  password=$(kubectl get secret argocd-initial-admin-secret -n argocd -o jsonpath='{.data.password}' | base64 -d)
  session=$(curl -fsS http://k3s:30080/api/v1/session -H 'Content-Type: application/json' \
    -d "{\"username\":\"admin\",\"password\":\"$password\"}" | jq -er .token)
  token=$(curl -fsS -X POST http://k3s:30080/api/v1/account/pipedash/token \
    -H "Authorization: Bearer $session" -H 'Content-Type: application/json' \
    -d '{"name":"pipedash"}' | jq -er .token)
  printf '%s' "$token" > /credentials/argocd-token
fi

until token=$(kubectl get secret pipedash-reader -n pipedash -o jsonpath='{.data.token}' | base64 -d) &&
  [ -n "$token" ]; do
  sleep 2
done
cat > /credentials/kubeconfig <<EOF
apiVersion: v1
kind: Config
clusters:
  - name: k3s
    cluster:
      server: https://k3s:6443
      certificate-authority-data: $(kubectl config view --raw -o jsonpath='{.clusters[0].cluster.certificate-authority-data}')
users:
  - name: pipedash-reader
    user:
      token: $token
contexts:
  - name: pipedash
    context:
      cluster: k3s
      user: pipedash-reader
current-context: pipedash
EOF

if [ -z "$(kubectl get pipelineruns -n acme-ci -o name)" ]; then
  kubectl create job demo-activity-seed --from=cronjob/demo-activity -n acme-ci
fi
