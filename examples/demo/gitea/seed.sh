#!/bin/sh
set -eu

gitea_cli() {
  su-exec git gitea --config /data/gitea/conf/app.ini "$@"
}

if ! gitea_cli admin user list | grep -qw acme; then
  gitea_cli admin user create --username acme --password "$GITEA_PASSWORD" \
    --email acme@example.com --admin --must-change-password=false
fi

if [ ! -s /shared/gitea-token ]; then
  token=$(gitea_cli admin user generate-access-token --username acme \
    --token-name "pipedash-$(date +%s)" \
    --scopes read:user,read:organization,read:repository --raw)
  printf '%s' "$token" > /shared/gitea-token
fi

if [ ! -s /shared/runner-token ]; then
  token=$(gitea_cli actions generate-runner-token)
  printf '%s' "$token" > /shared/runner-token
fi

for repo in /seed/repos/*; do
  name=$(basename "$repo")
  remote="http://acme:$GITEA_PASSWORD@gitea:3000/acme/$name.git"
  if git ls-remote "$remote" > /dev/null 2>&1; then
    continue
  fi
  work=$(mktemp -d)
  cp -R "$repo/." "$work"
  git -C "$work" init -q -b main
  git -C "$work" add .
  git -C "$work" -c user.name=acme -c user.email=acme@example.com commit -q -m "Initial commit"
  git -C "$work" push -q "$remote" main
  rm -rf "$work"
done
