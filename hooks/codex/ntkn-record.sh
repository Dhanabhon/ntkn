#!/usr/bin/env bash
# ntkn Codex hook: record token usage after each turn (Stop event).
#
# Reads transcript_path from hook JSON on stdin, finds all token_count events
# newer than the last recorded timestamp, aggregates by model, and calls
# `ntkn record`.
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
hook_model=$(jq -r '.model // empty' <<<"$input")

if [[ -z "$transcript" || ! -f "$transcript" ]]; then
  finish
fi

if [[ -z "$project_dir" || ! -d "$project_dir" ]]; then
  finish
fi

if [[ -z "$session_id" ]]; then
  finish
fi

rules="$project_dir/.ntkn/rules/ntkn-rules.md"
legacy_rules="$project_dir/.agents/rules/ntkn-rules.md"
state="$project_dir/.ntkn/codex-state.json"
legacy_state="$project_dir/.agents/ntkn-codex-state.json"

if [[ ! -f "$rules" && -f "$legacy_rules" ]]; then
  rules="$legacy_rules"
fi

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

mkdir -p "$(dirname "$state")"
if [[ ! -s "$state" && -s "$legacy_state" ]]; then
  cp "$legacy_state" "$state"
fi
if [[ ! -s "$state" ]]; then
  echo '{"sessions":{}}' >"$state"
fi

previous_ts=$(
  jq -r --arg sid "$session_id" '
    .sessions[$sid].last_timestamp // empty
  ' "$state"
)

snapshot=$(
  jq -s -c --arg prev_ts "$previous_ts" '
    {
      contexts: [.[] | select(.type == "turn_context") | {ts: .timestamp, model: (.payload.model // "unknown")}],
      events: [.[]
        | select(.type == "event_msg")
        | select(.payload.type == "token_count")
        | select($prev_ts == "" or .timestamp > $prev_ts)]
    }
  ' "$transcript" 2>/dev/null || true
)

if [[ -z "$snapshot" || "$snapshot" == "null" ]]; then
  finish
fi

event_count=$(jq -r '.events | length' <<<"$snapshot")
if [[ "$event_count" == "0" ]]; then
  finish
fi

default_model=$hook_model
if [[ -z "$default_model" || "$default_model" == "null" ]]; then
  default_model=$(
    jq -r '.contexts[-1].model // "unknown"' <<<"$snapshot"
  )
fi

aggregated=$(
  jq -c --arg default_model "$default_model" '
    . as $root
    | def model_for($ts):
        ([$root.contexts[] | select(.ts <= $ts)] | last.model) // $default_model;
    [$root.events[]
      | . as $event
      | ($event.payload.info.last_token_usage // {}) as $usage
      | {
          model: model_for($event.timestamp),
          prompt: (($usage.input_tokens // 0) + ($usage.cached_input_tokens // 0)),
          completion: (($usage.output_tokens // 0) + ($usage.reasoning_output_tokens // 0))
        }]
    | group_by(.model)
    | map({
        model: .[0].model,
        prompt: (map(.prompt) | add),
        completion: (map(.completion) | add)
      })
    | map(select((.prompt // 0) > 0 or (.completion // 0) > 0))
  ' <<<"$snapshot"
)

latest_ts=$(
  jq -r '[.events[].timestamp] | max' <<<"$snapshot"
)

if [[ -n "$aggregated" && "$aggregated" != "[]" && "$aggregated" != "null" ]]; then
  while IFS= read -r row; do
    [[ -z "$row" ]] && continue
    model=$(jq -r '.model' <<<"$row")
    prompt=$(jq -r '.prompt' <<<"$row")
    completion=$(jq -r '.completion' <<<"$row")

    if [[ "$prompt" == "0" && "$completion" == "0" ]]; then
      continue
    fi

    (
      cd "$project_dir" || exit 0
      ntkn record \
        --project "$project_id" \
        --model "$model" \
        --prompt "$prompt" \
        --comp "$completion" \
        >/dev/null 2>&1 || true
    )
  done < <(jq -c '.[]' <<<"$aggregated")
fi

tmp="${state}.tmp"
jq --arg sid "$session_id" --arg ts "$latest_ts" '
  .sessions[$sid].last_timestamp = $ts
' "$state" >"$tmp" && mv "$tmp" "$state"

finish
