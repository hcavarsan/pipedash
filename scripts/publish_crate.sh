#!/usr/bin/env bash
set -eo pipefail
crate_path="$1"
crate_name=$(basename "$crate_path")
max_retries=3
retry_delay=30
publish_args=()
if [ "${3:-}" = "allow-dirty" ]; then
  publish_args+=(--allow-dirty)
fi

# Wait 60s between publishes to avoid rate limiting
if [ "${2:-}" = "wait" ]; then
  echo "Waiting 60s to avoid rate limiting..."
  sleep 60
fi

echo "Publishing $crate_name..."
cd "$crate_path"

version=$(cargo metadata --no-deps --format-version 1 | jq -r --arg n "$crate_name" '.packages[] | select(.name == $n) | .version')
if curl -sf -A "pipedash-release" "https://crates.io/api/v1/crates/$crate_name/$version" >/dev/null; then
  echo "$crate_name $version already published, skipping"
  exit 0
fi

for attempt in $(seq 1 "$max_retries"); do
  echo "Attempt $attempt/$max_retries for $crate_name"

  if cargo publish "${publish_args[@]}"; then
    echo "Successfully published $crate_name"
    exit 0
  fi

  if [ "$attempt" -lt "$max_retries" ]; then
    echo "Retrying $crate_name in ${retry_delay}s..."
    sleep "$retry_delay"
    retry_delay=$((retry_delay * 2))
  else
    echo "Failed to publish $crate_name after $max_retries attempts"
    exit 1
  fi
done
