#!/usr/bin/env bash
set -euo pipefail
repo=$(cd "$(dirname "$0")/../.." && pwd)
exec "$repo/target/debug/mtx" evaluate --task atomic-backrun \
  --release "${1:?Provide a frozen continuous-arrival release}" \
  --agent codex --model openai/gpt-6.1-terra --effort high \
  --jobs-dir "$repo/work/backrun-codex-terra/jobs" \
  --job-name "${2:-terra-continuous-$(date +%Y%m%d-%H%M%S)}"
