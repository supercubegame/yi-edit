#!/usr/bin/env bash
# 快闸门。零依赖、不碰网络、不编 GUI（egui 一编就是几分钟，每轮都编就没人愿意跑它了）。
# 不用管道：`cmd | tee` 会把 cmd 的退出码吃掉，而那是一条会自己变绿的假绿。
set -Eeuo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$ROOT"

LOG=${GATE_LOG:-gate.log}
METRICS=${GATE_METRICS:-gate-metrics.txt}
: > "$LOG"
: > "$METRICS"

pass=0
fail=0
failed=()

run() {
  local name=$1
  shift
  printf '\n===== %s =====\n' "$name" >> "$LOG"
  if "$@" >> "$LOG" 2>&1; then
    pass=$((pass + 1))
    printf 'PASS: %s\n' "$name" >> "$LOG"
    printf 'PASS %s\n' "$name"
  else
    fail=$((fail + 1))
    failed+=("$name")
    printf 'FAIL: %s\n' "$name" >> "$LOG"
    printf 'FAIL %s\n' "$name"
  fi
}

# ---- 阻断项 ----
run "core/fileio/meta tests" cargo test -p yi-edit-core -p yi-edit-fileio -p yi-edit-meta
run "shotcheck selftest" bash -c 'cargo run -q -p yi-edit --bin yi-shotcheck -- --selftest'

# ---- 参考项（不阻断，但必须把实测值报出来）----
# fmt 目前不阻断，因为首轮代码是手写的。这笔欠账登记在 docs/OBLIGATIONS.md，
# 带期限，到期判红 —— 而不是「下次顺手做」。
fmt_out=$(cargo fmt --all -- --check 2>&1 || true)
fmt_files=$(printf '%s\n' "$fmt_out" | grep -c '^Diff in ' || true)
printf 'fmt_diff_lines=%s\n' "${fmt_files:-0}" >> "$METRICS"
printf '\n===== cargo fmt (advisory) =====\n%s\n' "$fmt_out" >> "$LOG"

printf 'gate_pass=%s\ngate_fail=%s\n' "$pass" "$fail" >> "$METRICS"
printf 'pass=%s\nfail=%s\nfailed=%s\n' "$pass" "$fail" "${failed[*]-none}" > gate-result.txt

echo
echo "----- gate.log -----"
cat "$LOG"
echo "----- summary -----"
cat gate-result.txt
cat "$METRICS"

if (( fail > 0 )); then
  exit 1
fi
