#!/usr/bin/env bash
# ntkn Codex hook: record token usage after each turn (Stop event).
#
# Reads transcript_path from hook JSON on stdin, finds the latest token_count
# event in the Codex session JSONL, and records its last_token_usage (per-turn).
#
# Requires: ntkn, jq
# Project setup: run `ntkn init --project <name>` once in the repo root.
#
# Codex Stop hooks must print JSON on stdout when exiting 0.

set -euo pipefail

finish() {
  echo '{"continue":true}'
  exit 0
}

input=$(cat)
transcript=$(jq -r '.transcript_path // empty' <<<"$input")
session_id=$(jq -r '.session_id // empty' <<<"$input")
project_dir=$(jq -r '.cwd // empty' <<<"$input")
model=$(jq -r '.model // empty' <<<"$input")

if [[ -z "$transcript" || ! -f "$transcript" ]]; then
  finish
fi

if [[ -z "$project_dir" || ! -d "$project_dir" ]]; then
  finish
fi

if [[ -z "$session_id" ]]; then
  finish
fi

rules="$project_dir/.agents/rules/ntkn-rules.md"
state="$project_dir/.agents/ntkn-codex-state.json"

if [[ ! -f "$rules" ]]; then
  finish
fi

if ! command -v ntkn >/dev/null 2>&1; then
  finish
fi

if ! command -v jq >/dev/null 2>&1; then
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

latest_event=$(
  jq -s -c '
    [.[]
     | select(.type == "event_msg")
     | select(.payload.type == "token_count")]
    | if length == 0 then empty else .[-1] end
  ' "$transcript" 2>/dev/null || true
)

if [[ -z "$latest_event" || "$latest_event" == "null" ]]; then
  finish
fi

event_ts=$(jq -r '.timestamp // empty' <<<"$latest_event")
last_usage=$(
  jq -c '.payload.info.last_token_usage // empty' <<<"$latest_event"
)

if [[ -z "$event_ts" || -z "$last_usage" || "$last_usage" == "null" ]]; then
  finish
fi

if [[ -z "$model" || "$model" == "null" ]]; then
  model=$(
    jq -s -r '
      [.[]
       | select(.type == "turn_context")
       | (.payload.model // .payload.turn_context.model // empty)
       | select(length > 0)]
      | last // "unknown"
    ' "$transcript" 2>/dev/null || echo "unknown"
  )
fi

mkdir -p "$(dirname "$state")"
if [[ ! -s "$state" ]]; then
  echo '{"sessions":{}}' >"$state"
fi

previous_ts=$(
  jq -r --arg sid "$session_id" '
    .sessions[$sid].last_timestamp // empty
  ' "$state"
)

if [[ "$event_ts" == "$previous_ts" ]]; then
  finish
fi

prompt=$(
  jq -r '
    (.input_tokens // 0) + (.cached_input_tokens // 0)
    | if . < 0 then 0 else . end
  ' <<<"$last_usage"
)
completion=$(
  jq -r '
    (.output_tokens // 0) + (.reasoning_output_tokens // 0)
    | if . < 0 then 0 else . end
  ' <<<"$last_usage"
)

if [[ "$prompt" != "0" || "$completion" != "0" ]]; then
  (
    cd "$project_dir" || exit 0
    ntkn record \
      --project "$project_id" \
      --model "$model" \
      --prompt "$prompt" \
      --comp "$completion" \
      >/dev/null 2>&1 || true
  )
fi

tmp="${state}.tmp"
jq --arg sid "$session_id" --arg ts "$event_ts" '
  .sessions[$sid].last_timestamp = $ts
' "$state" >"$tmp" && mv "$tmp" "$state"

finish
