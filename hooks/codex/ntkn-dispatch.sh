#!/usr/bin/env bash
# ntkn global Codex dispatcher: forwards Stop hook input to the project hook
# when the current workspace has been initialized with `ntkn init`.
#
# Installed to ~/.codex/hooks/ntkn-dispatch.sh by `ntkn init`.
# Codex Desktop has no /hooks command; use `ntkn sync-codex` after Codex work.

set -euo pipefail

finish() {
  echo '{"continue":true}'
  exit 0
}

input=$(cat)

if ! command -v jq >/dev/null 2>&1; then
  finish
fi

transcript=$(jq -r '.transcript_path // .transcriptPath // empty' <<<"$input")
project_dir=$(jq -r '.cwd // .working_dir // .workingDirectory // empty' <<<"$input")

if [[ -z "$project_dir" || ! -d "$project_dir" ]]; then
  project_dir=$(
    jq -r 'select(.type == "session_meta" or .type == "turn_context") | .payload.cwd // empty' "$transcript" 2>/dev/null \
      | awk 'NF { print; exit }' || true
  )
fi

if [[ -z "$project_dir" || ! -d "$project_dir" ]]; then
  finish
fi

hook="$project_dir/.ntkn/hooks/codex/ntkn-record.sh"
rules="$project_dir/.ntkn/rules/ntkn-rules.md"

if [[ ! -f "$hook" ]]; then
  hook="$project_dir/.agents/hooks/codex/ntkn-record.sh"
fi
if [[ ! -f "$rules" ]]; then
  rules="$project_dir/.agents/rules/ntkn-rules.md"
fi

if [[ ! -f "$rules" || ! -f "$hook" ]]; then
  finish
fi

echo "$input" | bash "$hook"
