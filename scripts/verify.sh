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

# 格式阻断。这一步替掉了之前的「上限棘轮」：欠账已经由 format 那条流水线清零，
# 从此它只能是 0。两条同时存在就是语义重复（上限 110 永远碰不到），
# 而一条永远为真的断言比没有断言更坏：它看起来像在守。
# fmt 的原始输出单独一份文件：往闸门日志里堆的话，它会把失败的测试名
# 全部挤出回写窗口（实测踩过，crates/meta/tests/gate.rs 里有负向断言守着）。
format_check() {
  cargo fmt --all -- --check > "${FMT_LOG}" 2>&1 || true
  n=$(grep -c '^Diff in ' "${FMT_LOG}" || true)
  n=${n:-0}
  printf 'fmt_diff_lines=%s rustfmt=%s\n' "${n}" "$(rustfmt --version 2>/dev/null || echo unknown)"
  if [ "${n}" -ne 0 ]; then
    printf 'FORMAT CHECK FAILED left: %s right: 0\n' "${n}"
    printf '不规范的文件（完整差异在 fmt.log 里）：\n'
    grep '^Diff in ' "${FMT_LOG}" | sed 's/^/  /' | head -n 40
    printf '修法：本地 cargo fmt --all，或者让 .github/workflows/format.yml 跑一遍（它会把结果回推）。\n'
    return 1
  fi
  printf '全部文件符合 rustfmt\n'
  return 0
}
run "format check" format_check

fmt_diff=$(grep -c '^Diff in ' "${FMT_LOG}" || true)
printf 'fmt_diff_lines=%s\n' "${fmt_diff:-0}" >> "${METRICS}"
# rustfmt 版本要进报告：阻断红了时，「我新敲了不规范的代码」与「上游换了格式规则」
# 在一个孤零的数字上长得一模一样。
printf 'rustfmt_version=%s\n' "$(rustfmt --version 2>/dev/null || echo unknown)" >> "${METRICS}"
printf 'gate_pass=%s\ngate_fail=%s\n' "${pass}" "${fail}" >> "${METRICS}"
printf 'pass=%s\nfail=%s\nfailed=%s\n' "${pass}" "${fail}" "${failed:-none}" > gate-result.txt
{
  printf '\n===== FAILURE SUMMARY =====\n'
  if [ "${fail}" -eq 0 ]; then printf 'no failures\n'; else
    # -A 4 是必需的：`panicked at ...` 的**下一行**才是断言消息。
    # 不带上下文的话，中文断言消息一条也到不了我手里（实测踩过），
    # 于是报告只能告诉我「哪条断言红了」而不是「为什么红了」。
    while read -r f; do [ -f "${f}" ] || continue; printf -- '--- %s: failing tests ---\n' "${f}"; grep -E -A 4 '(FAILED|panicked at|^error|FORMAT CHECK FAILED)' "${f}" | tail -n 80 || true; done < gate-failed-steps.txt
  fi
} >> "${LOG}"
echo "----- summary -----"; cat gate-result.txt; cat "${METRICS}"; echo "----- gate.log tail -----"; tail -n 60 "${LOG}"
[ "${fail}" -eq 0 ]
