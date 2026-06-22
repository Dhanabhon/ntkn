#!/usr/bin/env bash
# ntkn Antigravity stop hook: record token usage from the stop payload.
#
# Antigravity stop hooks send per-turn token fields on stdin:
#   input_tokens, output_tokens, cache_read_tokens, cache_write_tokens
# Transcripts do not include usage; the stop payload is the source of truth.
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

conversation_id=$(
  jq -r '.conversation_id // .conversationId // empty' <<<"$input" 2>/dev/null || true
)
generation_id=$(
  jq -r '.generation_id // .generationId // empty' <<<"$input" 2>/dev/null || true
)
message_id=$(
  jq -r '.message_id // .messageId // .event.message_id // .event.messageID // empty' <<<"$input" 2>/dev/null || true
)
request_id=$(
  jq -r '.request_id // .requestId // .event.request_id // .event.requestID // empty' <<<"$input" 2>/dev/null || true
)
turn_id=$(
  jq -r '.turn_id // .turnId // .event.turn_id // .event.turnID // empty' <<<"$input" 2>/dev/null || true
)
session_id=$(
  jq -r '.session_id // .sessionId // empty' <<<"$input" 2>/dev/null || true
)
project_dir=$(
  jq -r '
    .cwd
    // .workspace.current_dir
    // .workspace_roots[0]
    // .workspace_path
    // .workspacePath
    // .project_dir
    // empty
  ' <<<"$input" 2>/dev/null || true
)
transcript=$(
  jq -r '.transcript_path // .transcriptPath // empty' <<<"$input" 2>/dev/null || true
)

if [[ -z "$project_dir" || ! -d "$project_dir" ]]; then
  project_dir="$PWD"
fi

if [[ -z "$session_id" ]]; then
  session_id="$conversation_id"
fi

if [[ -z "$session_id" && -n "$transcript" ]]; then
  session_id=$(basename "$transcript" .jsonl)
fi

rules="$project_dir/.ntkn/rules/ntkn-rules.md"
legacy_rules="$project_dir/.agents/rules/ntkn-rules.md"
state="$project_dir/.ntkn/agy-state.json"
legacy_state="$project_dir/.agents/ntkn-agy-state.json"

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

model=$(
  jq -r '
    if (.model | type) == "string" then .model
    elif (.model | type) == "object" then (.model.id // .model.display_name // empty)
    else empty end
    // .usage.model
    // .token_usage.model
    // .tokenUsage.model
    // empty
  ' <<<"$input" 2>/dev/null || true
)
model=${model:-${NTKN_MODEL:-agy}}

prompt=""
completion=""
duration=$(
  jq -r '.duration_ms // .durationMs // .usage.duration_ms // .usage.durationMs // 0' \
    <<<"$input" 2>/dev/null || true
)
duration=${duration:-${NTKN_DURATION_MS:-0}}

extract_usage_object() {
  jq -r '
    def num($field):
      (.[$field] // empty | select(type == "number") | tostring);
    (
      num("input_tokens")
      // num("inputTokens")
      // num("prompt_tokens")
      // num("promptTokens")
      // "0"
    ) as $prompt
    | (
      num("completion_tokens")
      // num("completionTokens")
      // (
        (
          num("output_tokens")
          // num("outputTokens")
          // "0"
          | tonumber
        ) - (
          num("reasoning_output_tokens")
          // num("reasoningOutputTokens")
          // num("reasoning_tokens")
          // num("reasoningTokens")
          // "0"
          | tonumber
        )
        | [., 0] | max | tostring
      )
    ) as $completion
    | "\($prompt | tonumber)|\($completion | tonumber)"
  ' <<<"$1" 2>/dev/null || echo "0|0"
}

visible_output_tokens() {
  local output="$1"
  local payload="$2"
  local reasoning
  reasoning=$(
    jq -r '
      .reasoning_output_tokens
      // .reasoningOutputTokens
      // .reasoning_tokens
      // .reasoningTokens
      // .usage.reasoning_output_tokens
      // .usage.reasoningOutputTokens
      // .usage.reasoning_tokens
      // .usage.reasoningTokens
      // 0
    ' <<<"$payload" 2>/dev/null || true
  )
  reasoning=${reasoning:-0}
  if [[ "$reasoning" =~ ^[0-9]+$ ]]; then
    local visible=$((output - reasoning))
    if [[ "$visible" -lt 0 ]]; then visible=0; fi
    printf '%s\n' "$visible"
  else
    printf '%s\n' "$output"
  fi
}

# Antigravity stop payload: input_tokens is already total prompt-side (incl. cache).
input_tokens=$(
  jq -r '.input_tokens // .inputTokens // empty' <<<"$input" 2>/dev/null || true
)
output_tokens=$(
  jq -r '.output_tokens // .outputTokens // empty' <<<"$input" 2>/dev/null || true
)
if [[ "$input_tokens" =~ ^[0-9]+$ && "$output_tokens" =~ ^[0-9]+$ ]]; then
  prompt="$input_tokens"
  completion=$(visible_output_tokens "$output_tokens" "$input")
fi

current_usage=$(
  jq -c '.context_window.current_usage // empty' <<<"$input" 2>/dev/null || true
)
if [[ -z "$prompt" || -z "$completion" ]] && [[ -n "$current_usage" && "$current_usage" != "null" ]]; then
  IFS='|' read -r prompt completion < <(extract_usage_object "$current_usage")
fi

if ! [[ "$prompt" =~ ^[0-9]+$ && "$completion" =~ ^[0-9]+$ ]]; then
  prompt=$(
    jq -r '
      .prompt_tokens
      // .promptTokens
      // .usage.prompt_tokens
      // .usage.promptTokens
      // .usage.input_tokens
      // .usage.inputTokens
      // empty
    ' <<<"$input" 2>/dev/null || true
  )
  completion=$(
    jq -r '
      .completion_tokens
      // .completionTokens
      // .usage.completion_tokens
      // .usage.completionTokens
      // empty
    ' <<<"$input" 2>/dev/null || true
  )
  if ! [[ "$completion" =~ ^[0-9]+$ ]]; then
    output_tokens=$(
      jq -r '
        .output_tokens
        // .outputTokens
        // .usage.output_tokens
        // .usage.outputTokens
        // empty
      ' <<<"$input" 2>/dev/null || true
    )
    if [[ "$output_tokens" =~ ^[0-9]+$ ]]; then
      completion=$(visible_output_tokens "$output_tokens" "$input")
    fi
  fi
fi

prompt=${prompt:-${NTKN_PROMPT_TOKENS:-}}
completion=${completion:-${NTKN_COMPLETION_TOKENS:-}}

mkdir -p "$(dirname "$state")"
if [[ ! -s "$state" && -s "$legacy_state" ]]; then
  cp "$legacy_state" "$state"
fi
if [[ ! -s "$state" ]]; then
  echo '{"sessions":{}}' >"$state"
fi

if [[ -n "$session_id" ]] && ! [[ "$prompt" =~ ^[0-9]+$ && "$completion" =~ ^[0-9]+$ ]]; then
  total_input=$(
    jq -r '.context_window.total_input_tokens // empty' <<<"$input" 2>/dev/null || true
  )
  total_output=$(
    jq -r '.context_window.total_output_tokens // empty' <<<"$input" 2>/dev/null || true
  )

  if [[ "$total_input" =~ ^[0-9]+$ && "$total_output" =~ ^[0-9]+$ ]]; then
    previous=$(
      jq -c --arg sid "$session_id" '.sessions[$sid] // {}' "$state" 2>/dev/null || echo '{}'
    )
    prev_input=$(jq -r '.last_input // 0' <<<"$previous")
    prev_output=$(jq -r '.last_output // 0' <<<"$previous")
    prompt=$((total_input - prev_input))
    completion=$((total_output - prev_output))
    if [[ "$prompt" -lt 0 ]]; then prompt=0; fi
    if [[ "$completion" -lt 0 ]]; then completion=0; fi
  fi
fi

if ! [[ "$prompt" =~ ^[0-9]+$ && "$completion" =~ ^[0-9]+$ && "$duration" =~ ^[0-9]+$ ]]; then
  finish
fi

if [[ "$prompt" == "0" && "$completion" == "0" ]]; then
  finish
fi

stable_key="${generation_id:-${message_id:-${request_id:-${turn_id:-}}}}"
dedupe_key="${stable_key:-${session_id}:${prompt}:${completion}:${model}}"
if [[ -z "${NTKN_FORCE_SYNC:-}" && -n "$dedupe_key" ]]; then
  seen=$(
    jq -r --arg key "$dedupe_key" '.seen_generations[$key] // empty' "$state" 2>/dev/null || true
  )
  if [[ "$seen" == "true" ]]; then
    finish
  fi
fi

(
  cd "$project_dir" || exit 0
  ntkn record \
    --project "$project_id" \
    --provider "agy" \
    --model "$model" \
    --prompt "$prompt" \
    --comp "$completion" \
    --duration "$duration" \
    >/dev/null 2>&1 || true
)

tmp="${state}.tmp"
payload_compact=$(jq -c '.' <<<"$input" 2>/dev/null || echo '{}')
printf '%s\n' "$payload_compact" >"$project_dir/.ntkn/agy-last-payload.json"
total_input=$(
  jq -r '.context_window.total_input_tokens // empty' <<<"$input" 2>/dev/null || true
)
total_output=$(
  jq -r '.context_window.total_output_tokens // empty' <<<"$input" 2>/dev/null || true
)
jq \
  --arg key "$dedupe_key" \
  --arg sid "${session_id:-}" \
  --arg ti "${total_input:-}" \
  --arg to "${total_output:-}" \
  '
    .seen_generations[$key] = true
    | if ($sid | length) > 0 then
        if ($ti | length) > 0 and ($ti | test("^[0-9]+$")) then
          .sessions[$sid].last_input = ($ti | tonumber)
        else . end
        | if ($to | length) > 0 and ($to | test("^[0-9]+$")) then
            .sessions[$sid].last_output = ($to | tonumber)
          else . end
      else . end
  ' "$state" >"$tmp" && mv "$tmp" "$state"

finish
