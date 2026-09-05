#!/usr/bin/env bash
set -Eeuo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$ROOT"

LOG=${GATE_LOG:-gate.log}
: > "$LOG"
exec > >(tee -a "$LOG") 2>&1

pass=0
fail=0
failed=()
run() {
  local name=$1
  shift
  printf '\n== %s ==\n' "$name"
  if "$@"; then
    pass=$((pass + 1))
    echo "PASS: $name"
  else
    fail=$((fail + 1))
    failed+=("$name")
    echo "FAIL: $name"
  fi
}

run "workspace tests" cargo test --workspace
run "format check" cargo fmt --all -- --check
run "AGENTS/CLAUDE identity" bash -c 'cmp -s AGENTS.md CLAUDE.md'
run "docs line budget" bash -c 'test "$(wc -l < AGENTS.md)" -le 200 && test "$(wc -l < CLAUDE.md)" -le 200'

printf '\nSUMMARY pass=%s fail=%s\n' "$pass" "$fail"
printf 'FAILED: %s\n' "${failed[*]-none}"
printf 'pass=%s\nfail=%s\n' "$pass" "$fail" > gate-result.txt

if (( fail > 0 )); then
  exit 1
fi
