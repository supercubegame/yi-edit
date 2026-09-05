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

# fmt 欠账的**上限**（棘轮）。它不是「fmt 已阶断」（那是 OB-1，判据是
# scripts/verify.sh 里出现 run "format check"，本步故意不叫那个名字），
# 而是「它不得再变差」：实测从 28 一路涨到 99，一个只写在文档里的欠账
# 会静默变大，而变大的过程每一轮都是绿的。
# 实测 99（run 33992024625），上限给 110：那点余量是给 rustfmt 版本漂的，
# 不是给新敲的代码的。上限与 docs/OBLIGATIONS.md 里的数字耦合，
# crates/meta/tests/obligations.rs 里有一条等号断言钉着两头。
FMT_CEILING=110

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

# fmt 的原始输出单独一份文件：往闸门日志里堆的话，它会把失败的测试名
# 全部挤出回写窗口（实测踩过，crates/meta/tests/gate.rs 里有负向断言守着）。
cargo fmt --all -- --check > "${FMT_LOG}" 2>&1 || true
fmt_diff=$(grep -c '^Diff in ' "${FMT_LOG}" || true)
fmt_diff=${fmt_diff:-0}
printf 'fmt_diff_lines=%s\n' "${fmt_diff}" >> "${METRICS}"
printf 'fmt_ceiling=%s\n' "${FMT_CEILING}" >> "${METRICS}"
# rustfmt 版本要进报告：上限假红时，「我新敲了不规范的代码」与「上游换了格式规则」
# 在一个孤零的数字上长得一模一样。
printf 'rustfmt_version=%s\n' "$(rustfmt --version 2>/dev/null || echo unknown)" >> "${METRICS}"

# 棘轮本身是一个真步骤，不是一行备注：写在文档里的上限不会防住任何人。
fmt_ceiling_check() {
  printf 'fmt_diff_lines=%s ceiling=%s rustfmt=%s\n' \
    "${fmt_diff}" "${FMT_CEILING}" "$(rustfmt --version 2>/dev/null || echo unknown)"
  if [ "${fmt_diff}" -gt "${FMT_CEILING}" ]; then
    printf 'FMT CEILING FAILED left: %s right: %s\n' "${fmt_diff}" "${FMT_CEILING}"
    printf 'fmt 欠账又变大了。两个正确反应：把新敲的代码格式好，或者确认是 rustfmt 换了版本\n'
    printf '（上面印了版本）并同时改 scripts/verify.sh 与 docs/OBLIGATIONS.md 两处。只改一头会被等号断言拓住。\n'
    return 1
  fi
  if [ "${fmt_diff}" -lt "${FMT_CEILING}" ]; then
    printf '可以收紧：实测 %s < 上限 %s\n' "${fmt_diff}" "${FMT_CEILING}"
  fi
  return 0
}
run "fmt ceiling" fmt_ceiling_check

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
