#!/usr/bin/env bash
# ntkn Codex hook: record token usage after each turn (Stop event).
#
# Reads transcript_path from hook JSON on stdin, diffs cumulative token_count
# events in the Codex session JSONL, and calls `ntkn record`.
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

latest=$(
  jq -s -c '
    [.[]
     | select(.type == "event_msg")
     | select(.payload.type == "token_count")
     | .payload.info.total_token_usage
     | {
         input: (.input_tokens // 0),
         cached: (.cached_input_tokens // 0),
         output: (.output_tokens // 0),
         reasoning: (.reasoning_tokens // 0)
       }]
    | if length == 0 then empty else .[-1] end
  ' "$transcript" 2>/dev/null || true
)

if [[ -z "$latest" || "$latest" == "null" ]]; then
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

previous=$(
  jq -c --arg sid "$session_id" '
    .sessions[$sid] // {input: 0, cached: 0, output: 0, reasoning: 0}
  ' "$state"
)

delta=$(
  jq -c --argjson latest "$latest" --argjson previous "$previous" '
    {
      input: (($latest.input // 0) - ($previous.input // 0)),
      cached: (($latest.cached // 0) - ($previous.cached // 0)),
      output: (($latest.output // 0) - ($previous.output // 0)),
      reasoning: (($latest.reasoning // 0) - ($previous.reasoning // 0))
    }
  '
)

prompt=$(
  jq -r '
    (.input + .cached)
    | if . < 0 then 0 else . end
  ' <<<"$delta"
)
completion=$(
  jq -r '
    (.output + .reasoning)
    | if . < 0 then 0 else . end
  ' <<<"$delta"
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
jq --arg sid "$session_id" --argjson latest "$latest" '
  .sessions[$sid] = $latest
' "$state" >"$tmp" && mv "$tmp" "$state"

finish
