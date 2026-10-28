#!/bin/sh
set -eu

core=http://komodo:9120

until curl -fs "$core/version" > /dev/null 2>&1; do sleep 2; done

jwt=$(curl -fsS "$core/auth/login/LoginLocalUser" -H 'Content-Type: application/json' \
  -d "{\"username\":\"admin\",\"password\":\"$KOMODO_PASSWORD\"}" | jq -er .data.jwt)

call() {
  curl -fsS "$core/$1" -H "Authorization: $jwt" -H 'Content-Type: application/json' -d "$2"
}

if [ ! -s /shared/komodo-api-secret ]; then
  credentials=$(call auth/manage/CreateApiKey '{"name":"pipedash","expires":0}')
  printf '%s' "$(echo "$credentials" | jq -er .key)" > /shared/komodo-api-key
  printf '%s' "$(echo "$credentials" | jq -er .secret)" > /shared/komodo-api-secret
fi

until [ "$(call read/ListServers '{}' | jq -r '.[] | select(.name == "demo") | .info.state')" = Ok ]; do
  sleep 2
done
server=$(call read/ListServers '{}' | jq -er '.[] | select(.name == "demo") | .id')

existing=$(call read/ListStacks '{"query":{}}' | jq -r '.[].name')
for file in /seed/stacks/*.yaml; do
  name=$(basename "$file" .yaml)
  if ! echo "$existing" | grep -qx "$name"; then
    call write/CreateStack "$(jq -n --arg name "$name" --arg server "$server" --rawfile compose "$file" \
      '{name: $name, config: {server_id: $server, file_contents: $compose}}')" > /dev/null
  fi
done

if ! call read/ListProcedures '{"query":{}}' | jq -e '.[] | select(.name == "deploy-stacks")' > /dev/null; then
  stacks=$(for file in /seed/stacks/*.yaml; do basename "$file" .yaml; done | jq -Rn '[inputs]')
  call write/CreateProcedure "$(jq -n --argjson stacks "$stacks" '{
    name: "deploy-stacks",
    config: {
      schedule_format: "Cron",
      schedule: "0 */10 * * * *",
      schedule_alert: false,
      failure_alert: false,
      stages: [{
        name: "Deploy",
        executions: [$stacks[] | {execution: {type: "DeployStack", params: {stack: .}}}]
      }]
    }
  }')" > /dev/null
  call execute/RunProcedure '{"procedure":"deploy-stacks"}' > /dev/null
fi
