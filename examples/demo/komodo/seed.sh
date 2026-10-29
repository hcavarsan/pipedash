#!/bin/sh
set -eu

core=http://komodo:9120

until curl -fs "$core/version" > /dev/null 2>&1; do sleep 2; done

jwt=$(curl -fsS "$core/auth/login/LoginLocalUser" -H 'Content-Type: application/json' \
  -d "{\"username\":\"admin\",\"password\":\"$KOMODO_PASSWORD\"}" | jq -er .data.jwt)

call() {
  curl -fsS "$core/$1" -H "Authorization: $jwt" -H 'Content-Type: application/json' -d "$2"
}

user_id='.id // (._id | if type == "object" then ."$oid" else . end)'

if [ ! -s /credentials/komodo-api-secret ]; then
  reader=$(call read/ListUsers '{}' | jq -r ".[] | select(.username == \"pipedash\") | $user_id")
  if [ -z "$reader" ]; then
    reader=$(call write/CreateServiceUser '{"username":"pipedash","description":"Read-only demo access"}' |
      jq -er "$user_id")
  fi
  credentials=$(call write/CreateApiKeyForServiceUser \
    "$(jq -n --arg user "$reader" '{user_id: $user, name: "pipedash", expires: 0}')")
  printf '%s' "$(echo "$credentials" | jq -er .key)" > /credentials/komodo-api-key
  printf '%s' "$(echo "$credentials" | jq -er .secret)" > /credentials/komodo-api-secret
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
