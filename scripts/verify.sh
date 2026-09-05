#!/usr/bin/env bash
# 快闸门。零依赖、不碰网络、不编 GUI（egui 一编就是几分钟，每轮都编就没人愿意跑它了）。
# 不用管道：`cmd | tee` 会把 cmd 的退出码吃掉，而那是一条会自己变绿的假绿。
#
# **失败详情必须在日志末尾。** 实测踩过：回写发的是末尾 N 行，而参考项（fmt）
# 的输出接在测试输出后面且足够长，把失败的测试名全部挤出了窗口。
# 于是评论里只看到「失败 1 项」而定位不到根因 —— 等于没有报告。
# 所以：参考项写 fmt.log（不进闸门日志），并在末尾重新追一遍失败那几步的输出。
# crates/meta/tests/gate.rs 里有断言守这两条。
set -Eeuo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "${ROOT}"

LOG=${GATE_LOG:-gate.log}
METRICS=${GATE_METRICS:-gate-metrics.txt}
FMT_LOG=fmt.log
: > "${LOG}"
: > "${METRICS}"
: > "${FMT_LOG}"
rm -f gate-step-*.log

pass=0
fail=0
failed=""
step=0

run() {
  name=$1
  shift
  step=$((step + 1))
  out="gate-step-${step}.log"
  printf '\n===== %s =====\n' "${name}" >> "${LOG}"
  if "$@" > "${out}" 2>&1; then
    pass=$((pass + 1))
    cat "${out}" >> "${LOG}"
    printf 'PASS: %s\n' "${name}" >> "${LOG}"
    printf 'PASS %s\n' "${name}"
  else
    fail=$((fail + 1))
    failed="${failed}${name};"
    cat "${out}" >> "${LOG}"
    printf 'FAIL: %s\n' "${name}" >> "${LOG}"
    printf 'FAIL %s\n' "${name}"
    echo "${out}" >> gate-failed-steps.txt
  fi
}

rm -f gate-failed-steps.txt
touch gate-failed-steps.txt

# ---- 阻断项 ----
# 按 crate 选，不用 --workspace：crates/app 里的 egui 不得进快闸门（meta 里有断言守这一条）。
run "core/fileio/meta tests" cargo test -p yi-edit-core -p yi-edit-fileio -p yi-edit-meta

# ---- 参考项（不阻断）----
# fmt 目前不阻断，欠账登记在 docs/OBLIGATIONS.md 的 OB-1，到期判红。
# 它的原始输出**不得**进闸门日志：它会把真正的失败挤出回写的窗口。
cargo fmt --all -- --check > "${FMT_LOG}" 2>&1 || true
fmt_files=$(grep -c '^Diff in ' "${FMT_LOG}" || true)
printf 'fmt_diff_lines=%s\n' "${fmt_files:-0}" >> "${METRICS}"

printf 'gate_pass=%s\ngate_fail=%s\n' "${pass}" "${fail}" >> "${METRICS}"
printf 'pass=%s\nfail=%s\nfailed=%s\n' "${pass}" "${fail}" "${failed:-none}" > gate-result.txt

# ---- 失败摘要：重新追到日志**末尾**，因为回写只带得走末尾那几十行 ----
{
  printf '\n===== FAILURE SUMMARY =====\n'
  if [ "${fail}" -eq 0 ]; then
    printf 'no failures\n'
  else
    while read -r f; do
      [ -f "${f}" ] || continue
      printf -- '--- tail of %s ---\n' "${f}"
      tail -n 40 "${f}"
    done < gate-failed-steps.txt
  fi
} >> "${LOG}"

echo
echo "----- summary -----"
cat gate-result.txt
cat "${METRICS}"
echo "----- gate.log tail -----"
tail -n 60 "${LOG}"

if [ "${fail}" -gt 0 ]; then
  exit 1
fi
