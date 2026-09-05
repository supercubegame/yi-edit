#!/usr/bin/env bash
# 把一份报告回写到我读得到的地方。两条通道：有 PR 写 PR 评论，没 PR 写提交评论。
# 两条都有去重（只在一条上实现 marker 去重，生产里看不出来：每次推送都是新 SHA）。
# 回写失败必须退非零：静默的回写失败与一切正常在面板上一模一样。
#
# 用法：report.sh <body.md> [marker 行号，默认 1]
# 每一种报告占一个 marker（主报告 / 快闸门原始日志 / 每个平台的原始日志），
# 否则它们会互相覆盖，而覆盖的表现是「那条日志本来就不存在」。
#
# **这个脚本跑在三个平台上，macOS 的 /bin/bash 是 3.2.57。** 两条已经踩过的规矩：
# 1. 变量先初始化（它带 set -u）。
# 2. **`$var` 不能紧跟非 ASCII 字符**：C 语言环境下 bash 3.2 会把那几个字节
#    当成变量名的一部分，报 unbound variable。一律用 ${var}。
# crates/meta/tests/portable_shell.rs 里两条都有扫描器守着。
set -Eeuo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "${ROOT}"

BODY_FILE=${1:?用法: report.sh <body.md> [marker 行号]}
MARKER_LINE=${2:-1}
# marker 只存一份（scripts/marker.txt）：两处必须逐字相同的字串不能拄两份。
MARKER=$(sed -n "${MARKER_LINE}p" scripts/marker.txt)
if [ -z "${MARKER}" ]; then
  echo "REPORT FAIL: marker.txt line ${MARKER_LINE} is empty" >&2
  exit 1
fi

REPO=${GITHUB_REPOSITORY:?}
SHA=${GITHUB_SHA:?}
RUN_ID=${GITHUB_RUN_ID:-local}
SHORT=$(echo "${SHA}" | cut -c1-7)

channel=""
existing=""
attempts=0
pr=""

BODY=$(printf '%s\n\n%s\n\n<sub>commit %s / run %s</sub>\n' \
  "${MARKER}" "$(cat "${BODY_FILE}")" "${SHORT}" "${RUN_ID}")

try() {
  n=0
  until "$@"; do
    n=$((n + 1))
    attempts=$((attempts + 1))
    if [ "${n}" -ge 3 ]; then
      echo "REPORT: gave up after ${n} attempts: $*" >&2
      return 1
    fi
    sleep 5
  done
  attempts=$((attempts + 1))
  return 0
}

# PR 号只有一份真身（scripts/pr-number.sh）：report 与 attest 各自解析的话，
# 一边写 PR 评论、另一边去提交评论上找，而那看起来像「报告没送达」。
pr=$(bash scripts/pr-number.sh)

find_existing() {
  gh api "$1" --paginate \
    --jq "[.[] | select(.body != null and (.body | contains(\"${MARKER}\")))] | .[0].id // empty" \
    2> /dev/null || true
}

if [ -n "${pr}" ]; then
  channel="pr#${pr}"
  existing=$(find_existing "repos/${REPO}/issues/${pr}/comments")
  if [ -n "${existing}" ]; then
    try gh api -X PATCH "repos/${REPO}/issues/comments/${existing}" -f body="${BODY}" > /dev/null
  else
    try gh api -X POST "repos/${REPO}/issues/${pr}/comments" -f body="${BODY}" > /dev/null
  fi
else
  channel="commit ${SHORT}"
  existing=$(find_existing "repos/${REPO}/commits/${SHA}/comments")
  if [ -n "${existing}" ]; then
    try gh api -X PATCH "repos/${REPO}/comments/${existing}" -f body="${BODY}" > /dev/null
  else
    try gh api -X POST "repos/${REPO}/commits/${SHA}/comments" -f body="${BODY}" > /dev/null
  fi
fi

# 尝试次数要进报告：否则「一次就过」与「第三次才过」长得一模一样，
# 上游在慢慢变差就看不见。这两行故意全 ASCII：它们里面有变量展开。
updated=no
if [ -n "${existing}" ]; then
  updated=yes
fi
echo "marker_line=${MARKER_LINE} channel=${channel} attempts=${attempts} updated_existing=${updated}" \
  >> report-attempts.txt
echo "REPORT: marker line ${MARKER_LINE} written to ${channel} (attempts=${attempts} reused=${updated})"
