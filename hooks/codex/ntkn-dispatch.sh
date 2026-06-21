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
project_dir=$(jq -r '.cwd // empty' <<<"$input")

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
