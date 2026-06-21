#!/usr/bin/env bash
# ntkn OpenCode plugin recorder: record token usage from session.idle events.
#
# Requires: ntkn, jq
# Project setup: run `ntkn init --project <name>` once in the repo root.

set -euo pipefail

finish() {
  echo '{}'
  exit 0
}

input=$(cat)

if ! command -v jq >/dev/null 2>&1; then
  finish
fi

if ! command -v ntkn >/dev/null 2>&1; then
  finish
fi

project_dir=$(
  jq -r '.cwd // .directory // .project_dir // .event.cwd // empty' <<<"$input" 2>/dev/null || true
)
if [[ -z "$project_dir" || ! -d "$project_dir" ]]; then
  project_dir="$PWD"
fi

rules="$project_dir/.ntkn/rules/ntkn-rules.md"
legacy_rules="$project_dir/.agents/rules/ntkn-rules.md"
state="$project_dir/.ntkn/opencode-state.json"
legacy_state="$project_dir/.agents/ntkn-opencode-state.json"

if [[ ! -f "$rules" && -f "$legacy_rules" ]]; then
  rules="$legacy_rules"
fi

if [[ ! -f "$rules" ]]; then
  finish
fi

project_id=$(
  awk -F: '
    /^project_id:/ {
      value = substr($0, index($0, ":") + 1)
      gsub(/^[ \t]+|[ \t]+$/, "", value)
      if (value ~ /^".*"$/) {
        value = substr(value, 2, length(value) - 2)
        gsub(/\\"/, "\"", value)
        gsub(/\\\\/, "\\", value)
      }
      print value
      exit
    }
  ' "$rules"
)

if [[ -z "$project_id" ]]; then
  finish
fi

session_id=$(
  jq -r '
    .event.properties.sessionID
    // .event.properties.session_id
    // .event.sessionID
    // .event.session_id
    // .sessionID
    // .session_id
    // empty
  ' <<<"$input" 2>/dev/null || true
)

message_id=$(
  jq -r '
    .event.properties.messageID
    // .event.properties.message_id
    // .event.messageID
    // .event.message_id
    // .messageID
    // .message_id
    // empty
  ' <<<"$input" 2>/dev/null || true
)

model=$(
  jq -r '
    [
      .. | objects
      | .model? // .modelID? // .model_id? // .modelName? // .model_name? // empty
      | if type == "object" then (.id // .name // empty) else . end
      | select(type == "string" and length > 0)
    ][0] // empty
  ' <<<"$input" 2>/dev/null || true
)
model=${model:-${NTKN_MODEL:-opencode}}

usage=$(
  jq -c '
    [
      .. | objects
      | {
          prompt: (
            .input_tokens
            // .inputTokens
            // .prompt_tokens
            // .promptTokens
            // .tokens.input
            // .tokens.prompt
            // .usage.input_tokens
            // .usage.inputTokens
            // .usage.prompt_tokens
            // .usage.promptTokens
            // empty
          ),
          completion: (
            .output_tokens
            // .outputTokens
            // .completion_tokens
            // .completionTokens
            // .tokens.output
            // .tokens.completion
            // .usage.output_tokens
            // .usage.outputTokens
            // .usage.completion_tokens
            // .usage.completionTokens
            // empty
          )
        }
      | select((.prompt | type) == "number" and (.completion | type) == "number")
    ][0] // empty
  ' <<<"$input" 2>/dev/null || true
)

prompt=$(jq -r '.prompt // empty' <<<"$usage" 2>/dev/null || true)
completion=$(jq -r '.completion // empty' <<<"$usage" 2>/dev/null || true)
duration=$(
  jq -r '[.. | objects | .duration_ms? // .durationMs? // empty | select(type == "number")][0] // 0' \
    <<<"$input" 2>/dev/null || true
)
duration=${duration:-${NTKN_DURATION_MS:-0}}

if ! [[ "$prompt" =~ ^[0-9]+$ && "$completion" =~ ^[0-9]+$ && "$duration" =~ ^[0-9]+$ ]]; then
  finish
fi

if [[ "$prompt" == "0" && "$completion" == "0" ]]; then
  finish
fi

mkdir -p "$(dirname "$state")"
if [[ ! -s "$state" && -s "$legacy_state" ]]; then
  cp "$legacy_state" "$state"
fi
if [[ ! -s "$state" ]]; then
  echo '{"seen":{}}' >"$state"
fi

dedupe_key="${message_id:-${session_id}:${model}:${prompt}:${completion}}"
if [[ -z "${NTKN_FORCE_SYNC:-}" && -n "$dedupe_key" ]]; then
  seen=$(jq -r --arg key "$dedupe_key" '.seen[$key] // empty' "$state" 2>/dev/null || true)
  if [[ "$seen" == "true" ]]; then
    finish
  fi
fi

(
  cd "$project_dir" || exit 0
  ntkn record \
    --project "$project_id" \
    --provider "opencode" \
    --model "$model" \
    --prompt "$prompt" \
    --comp "$completion" \
    --duration "$duration" \
    >/dev/null 2>&1 || true
)

tmp="${state}.tmp"
jq --arg key "$dedupe_key" '.seen[$key] = true' "$state" >"$tmp" && mv "$tmp" "$state"
printf '%s\n' "$(jq -c '.' <<<"$input" 2>/dev/null || echo '{}')" >"$project_dir/.ntkn/opencode-last-event.json"

finish
