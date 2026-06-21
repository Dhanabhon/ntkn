#!/usr/bin/env bash
# ntkn Claude Code hook: record token usage after each turn (Stop event).
#
# Reads the session transcript_path from hook JSON on stdin, finds assistant
# messages not yet recorded for this session, and calls `ntkn record`.
#
# Requires: ntkn, jq
# Project setup: run `ntkn init --project <name>` once in the repo root.

set -euo pipefail

input=$(cat)
transcript=$(jq -r '.transcript_path // empty' <<<"$input")
session_id=$(jq -r '.session_id // empty' <<<"$input")
project_dir=$(jq -r '.cwd // empty' <<<"$input")

if [[ -n "${CLAUDE_PROJECT_DIR:-}" ]]; then
  project_dir="$CLAUDE_PROJECT_DIR"
fi

if [[ -z "$transcript" || ! -f "$transcript" ]]; then
  exit 0
fi

if [[ -z "$project_dir" || ! -d "$project_dir" ]]; then
  exit 0
fi

if [[ -z "$session_id" ]]; then
  exit 0
fi

rules="$project_dir/.agents/rules/ntkn-rules.md"
state="$project_dir/.agents/ntkn-claude-state.json"

if [[ ! -f "$rules" ]]; then
  exit 0
fi

if ! command -v ntkn >/dev/null 2>&1; then
  exit 0
fi

if ! command -v jq >/dev/null 2>&1; then
  exit 0
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
  exit 0
fi

mkdir -p "$(dirname "$state")"
if [[ ! -s "$state" ]]; then
  echo '{"sessions":{}}' >"$state"
fi

seen=$(jq -c --arg sid "$session_id" '.sessions[$sid].seen_uuids // []' "$state")

batch=$(
  jq -s -c --argjson seen "$seen" '
    [.[]
     | select(.type == "assistant")
     | select(.message.usage != null)
     | select((.isSidechain // false) | not)
     | select((.isApiErrorMessage // false) | not)
     | select(.uuid as $uuid | ($seen | contains([$uuid]) | not))
     | {
         uuid: .uuid,
         model: (.message.model // "unknown"),
         prompt: (
           (.message.usage.input_tokens // 0)
           + (.message.usage.cache_read_input_tokens // 0)
           + (.message.usage.cache_creation_input_tokens // 0)
         ),
         completion: (.message.usage.output_tokens // 0)
       }]
  ' "$transcript" 2>/dev/null || echo '[]'
)

if [[ "$batch" == "[]" ]]; then
  exit 0
fi

aggregated=$(
  jq -c '
    group_by(.model)
    | map({
        model: .[0].model,
        prompt: (map(.prompt) | add),
        completion: (map(.completion) | add),
        uuids: (map(.uuid))
      })
  ' <<<"$batch"
)

while IFS= read -r row; do
  [[ -z "$row" ]] && continue
  model=$(jq -r '.model' <<<"$row")
  prompt=$(jq -r '.prompt' <<<"$row")
  completion=$(jq -r '.completion' <<<"$row")

  if [[ "$prompt" == "0" && "$completion" == "0" ]]; then
    continue
  fi

  ntkn record \
    --project "$project_id" \
    --provider "claude-code" \
    --model "$model" \
    --prompt "$prompt" \
    --comp "$completion" \
    >/dev/null 2>&1 || true
done < <(jq -c '.[]' <<<"$aggregated")

new_uuids=$(jq -c '[.[].uuid]' <<<"$batch")
tmp="${state}.tmp"
jq --arg sid "$session_id" --argjson uuids "$new_uuids" '
  .sessions[$sid].seen_uuids = ((.sessions[$sid].seen_uuids // []) + $uuids | unique)
' "$state" >"$tmp" && mv "$tmp" "$state"

exit 0
