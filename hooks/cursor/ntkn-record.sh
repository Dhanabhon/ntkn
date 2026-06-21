#!/usr/bin/env bash
# ntkn Cursor hook: records token usage when Cursor exposes usage fields.
#
# Cursor project hooks run from the project root. Current Cursor transcripts do
# not consistently expose token counts, so this hook records only when the hook
# payload or environment includes real usage values.

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
  jq -r '.cwd // .workspace_path // .workspacePath // .project_dir // empty' <<<"$input" 2>/dev/null \
    || true
)

if [[ -z "$project_dir" || ! -d "$project_dir" ]]; then
  project_dir="$PWD"
fi

rules="$project_dir/.ntkn/rules/ntkn-rules.md"
legacy_rules="$project_dir/.agents/rules/ntkn-rules.md"

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
  jq -r '.model // .usage.model // .token_usage.model // .tokenUsage.model // empty' \
    <<<"$input" 2>/dev/null || true
)
prompt=$(
  jq -r '
    .prompt_tokens
    // .promptTokens
    // .usage.prompt_tokens
    // .usage.promptTokens
    // .usage.input_tokens
    // .usage.inputTokens
    // .token_usage.prompt_tokens
    // .token_usage.input_tokens
    // .tokenUsage.promptTokens
    // .tokenUsage.inputTokens
    // empty
  ' <<<"$input" 2>/dev/null || true
)
completion=$(
  jq -r '
    .completion_tokens
    // .completionTokens
    // .usage.completion_tokens
    // .usage.completionTokens
    // .usage.output_tokens
    // .usage.outputTokens
    // .token_usage.completion_tokens
    // .token_usage.output_tokens
    // .tokenUsage.completionTokens
    // .tokenUsage.outputTokens
    // empty
  ' <<<"$input" 2>/dev/null || true
)
duration=$(
  jq -r '.duration_ms // .durationMs // .usage.duration_ms // .usage.durationMs // 0' \
    <<<"$input" 2>/dev/null || true
)

model=${model:-${NTKN_MODEL:-cursor}}
prompt=${prompt:-${NTKN_PROMPT_TOKENS:-}}
completion=${completion:-${NTKN_COMPLETION_TOKENS:-}}
duration=${duration:-${NTKN_DURATION_MS:-0}}

if ! [[ "$prompt" =~ ^[0-9]+$ && "$completion" =~ ^[0-9]+$ && "$duration" =~ ^[0-9]+$ ]]; then
  finish
fi

if [[ "$prompt" == "0" && "$completion" == "0" ]]; then
  finish
fi

(
  cd "$project_dir" || exit 0
  ntkn record \
    --project "$project_id" \
    --model "$model" \
    --prompt "$prompt" \
    --comp "$completion" \
    --duration "$duration" \
    >/dev/null 2>&1 || true
)

finish
