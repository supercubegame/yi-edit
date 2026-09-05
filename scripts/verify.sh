#!/usr/bin/env bash
# 快闸门。零依赖、不碰网络、不编 GUI（egui 一编就是几分钟，每轮都编它就没人愿意跑了）。
# 不用管道：`cmd | tee` 会把 cmd 的退出码吃掉，而那是一条会自己变绿的假绿。
# 两个实测踩过的坑：失败详情要在日志末尾；cargo test 要关 fail-fast。
set -Eeuo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "${ROOT}"
LOG=${GATE_LOG:-gate.log}
METRICS=${GATE_METRICS:-gate-metrics.txt}
FMT_LOG=fmt.log
: > "${LOG}"; : > "${METRICS}"; : > "${FMT_LOG}"
rm -f gate-step-*.log gate-failed-steps.txt; touch gate-failed-steps.txt
pass=0; fail=0; failed=""; step=0
run() {
  name=$1; shift; step=$((step + 1)); out="gate-step-${step}.log"
  printf '\n===== %s =====\n' "${name}" >> "${LOG}"
  if "$@" > "${out}" 2>&1; then
    pass=$((pass + 1)); cat "${out}" >> "${LOG}"; printf 'PASS: %s\n' "${name}" >> "${LOG}"; printf 'PASS %s\n' "${name}"
  else
    fail=$((fail + 1)); failed="${failed}${name};"; cat "${out}" >> "${LOG}"; printf 'FAIL: %s\n' "${name}" >> "${LOG}"; printf 'FAIL %s\n' "${name}"; echo "${out}" >> gate-failed-steps.txt
  fi
}
run "core/fileio/session/meta tests" cargo test --no-fail-fast -p yi-edit-core -p yi-edit-fileio -p yi-edit-session -p yi-edit-meta
cargo fmt --all -- --check > "${FMT_LOG}" 2>&1 || true
fmt_files=$(grep -c '^Diff in ' "${FMT_LOG}" || true)
printf 'fmt_diff_lines=%s\n' "${fmt_files:-0}" >> "${METRICS}"
printf 'gate_pass=%s\ngate_fail=%s\n' "${pass}" "${fail}" >> "${METRICS}"
printf 'pass=%s\nfail=%s\nfailed=%s\n' "${pass}" "${fail}" "${failed:-none}" > gate-result.txt
{
  printf '\n===== FAILURE SUMMARY =====\n'
  if [ "${fail}" -eq 0 ]; then printf 'no failures\n'; else
    while read -r f; do [ -f "${f}" ] || continue; printf -- '--- %s: failing tests ---\n' "${f}"; grep -E '(FAILED|panicked at|assertion|^error|left:|right:|^ *[a-z_]+$)' "${f}" | tail -n 60 || true; done < gate-failed-steps.txt
  fi
} >> "${LOG}"
echo "----- summary -----"; cat gate-result.txt; cat "${METRICS}"; echo "----- gate.log tail -----"; tail -n 60 "${LOG}"
[ "${fail}" -eq 0 ]
